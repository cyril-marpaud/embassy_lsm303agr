#![no_std]
#![no_main]

mod fmt;

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_nrf::{
	bind_interrupts,
	gpio::{AnyPin, Pin},
	peripherals::TWISPI0,
	twim::{Config, InterruptHandler, Twim},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Ticker};
use fmt::{info, trace};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
	SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0 => InterruptHandler<TWISPI0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
	let p = embassy_nrf::init(Default::default());

	fmt::unwrap!(spawner.spawn(measure_task(
		p.TWISPI0,
		p.P0_16.degrade(),
		p.P0_08.degrade(),
	)));

	fmt::unwrap!(spawner.spawn(display_task()));
}

type Measure = (i32, i32, i32);

static CHANNEL: Channel<CriticalSectionRawMutex, Measure, 2000> = Channel::new();

#[embassy_executor::task]
async fn display_task() {
	let mut per_sec = 0;
	let mut ticker = Ticker::every(Duration::from_secs(1));

	loop {
		if let Either::First(received) = select(CHANNEL.receive(), ticker.next()).await {
			trace!("received: {}", received);
			per_sec += 1
		} else {
			info!("per sec: {}", per_sec);
			per_sec = 0
		}
	}
}

#[embassy_executor::task]
async fn measure_task(twim: TWISPI0, sda: AnyPin, scl: AnyPin) {
	let twim = Twim::new(twim, Irqs, sda, scl, Config::default());

	let mut sensor = lsm303agr::Lsm303agr::new_with_i2c(twim);
	sensor.init().await.unwrap();

	let Ok(mut sensor) = sensor.into_mag_continuous().await else {
		panic!("into mag continuous error");
	};

	loop {
		if sensor.mag_status().await.unwrap().xyz_new_data() {
			CHANNEL
				.send(sensor.magnetic_field().await.unwrap().xyz_nt())
				.await;
		}
	}
}
