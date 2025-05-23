// Example of blocking TSC (Touch Sensing Controller) that lights an LED when touch is detected.
//
// This example demonstrates:
// 1. Configuring a single TSC channel pin
// 2. Using the blocking TSC interface with polling
// 3. Waiting for acquisition completion using `poll_for_acquisition`
// 4. Reading touch values and controlling an LED based on the results
//
// Suggested physical setup on STM32L073RZ Nucleo board:
// - Connect a 1000pF capacitor between pin PA0 and GND. This is your sampling capacitor.
// - Connect one end of a 1K resistor to pin PA1 and leave the other end loose.
//   The loose end will act as the touch sensor which will register your touch.
//
// The example uses two pins from Group 1 of the TSC on the STM32L073RZ Nucleo board:
// - PA0 as the sampling capacitor, TSC group 1 IO1 (label A0)
// - PA1 as the channel pin, TSC group 1 IO2 (label A1)
//
// The program continuously reads the touch sensor value:
// - It starts acquisition, waits for completion using `poll_for_acquisition`, and reads the value.
// - The LED is turned on when touch is detected (sensor value < 25).
// - Touch values are logged to the console.
//
// Troubleshooting:
// - If touch is not detected, try adjusting the SENSOR_THRESHOLD value.
// - Experiment with different values for ct_pulse_high_length, ct_pulse_low_length,
//   pulse_generator_prescaler, max_count_value, and discharge_delay to optimize sensitivity.
//
// Note: Configuration values and sampling capacitor value have been determined experimentally.
// Optimal values may vary based on your specific hardware setup.
// Pins have been chosen for their convenient locations on the STM32L073RZ board. Refer to the
// official relevant STM32 datasheets and nucleo-board user manuals to find suitable
// alternative pins.
//
// Beware for STM32L073RZ nucleo-board, that PA2 and PA3 is used for the uart connection to
// the programmer chip. If you try to use these two pins for TSC, you will get strange
// readings, unless you somehow reconfigure/re-wire your nucleo-board.
// No errors or warnings will be emitted, they will just silently not work as expected.
// (see nucleo user manual UM1724, Rev 14, page 25)

#![no_std]
#![no_main]

use defmt::*;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::tsc::{self, *};
use embassy_stm32::{mode, peripherals};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

const SENSOR_THRESHOLD: u16 = 25; // Adjust this value based on your setup

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let device_config = embassy_stm32::Config::default();
    let context = embassy_stm32::init(device_config);

    let tsc_conf = Config {
        ct_pulse_high_length: ChargeTransferPulseCycle::_4,
        ct_pulse_low_length: ChargeTransferPulseCycle::_4,
        spread_spectrum: false,
        spread_spectrum_deviation: SSDeviation::new(2).unwrap(),
        spread_spectrum_prescaler: false,
        pulse_generator_prescaler: PGPrescalerDivider::_16,
        max_count_value: MaxCount::_255,
        io_default_mode: false,
        synchro_pin_polarity: false,
        acquisition_mode: false,
        max_count_interrupt: false,
    };

    let mut g1: PinGroupWithRoles<peripherals::TSC, G1> = PinGroupWithRoles::default();
    g1.set_io1::<tsc::pin_roles::Sample>(context.PA0);
    let tsc_sensor = g1.set_io2::<tsc::pin_roles::Channel>(context.PA1);

    let pin_groups: PinGroups<peripherals::TSC> = PinGroups {
        g1: Some(g1.pin_group),
        ..Default::default()
    };

    let mut touch_controller = tsc::Tsc::new_blocking(context.TSC, pin_groups, tsc_conf).unwrap();

    // Check if TSC is ready
    if touch_controller.get_state() != State::Ready {
        crate::panic!("TSC not ready!");
    }
    info!("TSC initialized successfully");

    // LED2 on the STM32L073RZ nucleo-board (PA5)
    let mut led = Output::new(context.PA5, Level::High, Speed::Low);

    // smaller sample capacitor discharge faster and can be used with shorter delay.
    let discharge_delay = 5; // ms

    // the interval at which the loop polls for new touch sensor values
    let polling_interval = 100; // ms

    info!("polling for touch");
    loop {
        touch_controller.set_active_channels_mask(tsc_sensor.pin.into());
        touch_controller.start();
        touch_controller.poll_for_acquisition();
        touch_controller.discharge_io(true);
        Timer::after_millis(discharge_delay).await;

        match read_touch_value(&mut touch_controller, tsc_sensor.pin).await {
            Some(v) => {
                info!("sensor value {}", v);
                if v < SENSOR_THRESHOLD {
                    led.set_high();
                } else {
                    led.set_low();
                }
            }
            None => led.set_low(),
        }

        Timer::after_millis(polling_interval).await;
    }
}

const MAX_GROUP_STATUS_READ_ATTEMPTS: usize = 10;

// attempt to read group status and delay when still ongoing
async fn read_touch_value(
    touch_controller: &mut tsc::Tsc<'_, peripherals::TSC, mode::Blocking>,
    sensor_pin: tsc::IOPin,
) -> Option<u16> {
    for _ in 0..MAX_GROUP_STATUS_READ_ATTEMPTS {
        match touch_controller.group_get_status(sensor_pin.group()) {
            GroupStatus::Complete => {
                return Some(touch_controller.group_get_value(sensor_pin.group()));
            }
            GroupStatus::Ongoing => {
                // if you end up here a lot, then you prob need to increase discharge_delay
                // or consider changing the code to adjust the discharge_delay dynamically
                info!("Acquisition still ongoing");
                Timer::after_millis(1).await;
            }
        }
    }
    None
}
