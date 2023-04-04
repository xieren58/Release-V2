#![no_std]
#![no_main]

//use defmt::println;
use defmt_rtt as _;
use panic_probe as _;
use stm32f3xx_hal as hal;

use hal::delay::Delay;
use hal::pac;
use hal::prelude::*;
use hal::spi::Spi;

use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use mfrc522::{Mfrc522, Uid};

#[cortex_m_rt::entry]
fn main() -> ! {

    //list of cards with access 
    let uid_card_pass_1 = [106, 205, 135,25];
    
    //peripheral stuff
    let dp = pac::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();   
    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut gpiob = dp.GPIOB.split(&mut rcc.ahb);       //  unsure if I need to set this pheriferal to communite through SWD
    
    
    // clock configuration
    let clocks = rcc
        .cfgr       // set internal clock by removing .use_hse(8.MHz())
        .sysclk(48.MHz())
        .pclk1(24.MHz())
        .freeze(&mut flash.acr);

    //led configuration
    let mut red_led =gpioa
        .pa1
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);
    
    let mut green_led = gpioa 
        .pa2
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);

    let mut on_off_5v_rail = gpioa 
        .pa3
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);


    //tim2 clock
    let tim2_channels = hal::pwm::tim2(
        dp.TIM2,
        1280,    // resolution of duty cycle
        50.Hz(), // frequency of period
        &clocks, // To get the timer's clock speed
    );

    //pwm servo pins and tim3

    let pa0 = gpioa 
        .pa0
        .into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);cd
    
    let mut tim2_ch1 = tim2_channels.0.output_to_pa0(pa0);
        
    // Configure pins for SPI
    
    let nss = gpioa //nss/sda =pd0
        .pa4
        .into_push_pull_output(&mut gpioa.moder, &mut gpioa.otyper);

    let sck = gpioa         //sck = pa5
        .pa5
        .into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl); //idk if i need this::<5> check datasheet p48


    let miso = gpioa            //miso = pa6
        .pa6
        .into_af_push_pull(&mut gpioa.moder,&mut gpioa.otyper,&mut gpioa.afrl);
        //into_af_push_pull(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrh);
    let mosi = gpioa            //mosi = pc12
        .pa7
        .into_af_push_pull::<5>(&mut gpioa.moder,&mut gpioa.otyper,&mut gpioa.afrl);
    
    // defaults to using MODE_0
    let spi = Spi::new(dp.SPI1, (sck, miso, mosi), 1.MHz(), clocks, &mut rcc.apb2);

    let mut timer = Delay::new(cp.SYST, clocks);

    let mut mfrc522 = Mfrc522::new(spi, nss).expect("could not create MFRC522");
    match mfrc522.version() {
        Ok(version) => defmt::info!("version {:x}", version),
        Err(_) => defmt::error!("version error"),
    }

    //this Loop needs understanding
    on_off_5v_rail.set_low().unwrap();
    
    let write = false;
    loop {
        //turning servo left and right 
        
        //verification for RFID 
        if let Ok(atqa) = mfrc522.wupa() {
                defmt::info!("new card detected");
                match mfrc522.select(&atqa) {

                    Ok(ref uid @ Uid::Single(ref inner)) => {
                        defmt::info!("card uid {=[?]}", inner.as_bytes());
                        defmt::info!("card uid {=[?]}", uid_card_pass_1);
                        handle_card(&mut mfrc522, &uid, write);
                        
                        if inner.as_bytes() == uid_card_pass_1 {
                            on_off_5v_rail.toggle().unwrap();
                            defmt::info!("unlocking and locking trolley!");
                            green_led.toggle().unwrap();
                            tim2_ch1.set_duty(tim2_ch1.get_max_duty()/10); // 5% duty cyle 90° 
                            tim2_ch1.enable();
                            cortex_m::asm::delay(10_000_000);
                            tim2_ch1.set_duty(tim2_ch1.get_max_duty()/20); // 10% duty cyle 180° 
                            tim2_ch1.enable();
                            cortex_m::asm::delay(5_000_000);
                            green_led.toggle().unwrap();
                            on_off_5v_rail.toggle().unwrap();
                        }
                        else{
                            for _ in 0..12 {
                                cortex_m::asm::delay(5_000_000);
                                red_led.toggle().unwrap();
                                cortex_m::asm::delay(5_000_000);
                                } 
                            }

                    }
                    Ok(ref uid @ Uid::Double(ref inner)) => {
                        defmt::info!("card double uid {=[?]}", inner.as_bytes());
                        handle_card(&mut mfrc522, &uid, write);
                    }
                    Ok(_) => {
                        defmt::info!("got other uid size")
                    }

                    Err(_) => {
                        //uid errors 
                        defmt::error!("Select error");

                    }
                }
            }
            timer.delay_ms(1000u32);
        }
        
        
}




fn handle_card<E, SPI, NSS>(mfrc522: &mut Mfrc522<SPI, NSS>, uid: &Uid, write: bool)
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    NSS: OutputPin,
{
    let key = [0xFF; 6];
    let buffer = [
        0xDE, 0x42, 0xAD, 0x42, 0xBE, 0x42, 0xEF, 0x42, 0xCA, 0x42, 0xFE, 0x42, 0xBA, 0x42, 0xBE,
        0x42,
    ];
    if mfrc522.mf_authenticate(uid, 1, &key).is_ok() {
        if write {
            match mfrc522.mf_write(1, buffer) {
                Ok(_) => {
                    defmt::info!("write success");
                }
                Err(_) => {
                    defmt::error!("error during read");
                
                }
            }
        } else {
            match mfrc522.mf_read(1) {
                Ok(data) => defmt::info!("read {=[?]}", data),
                Err(_) => {
                    defmt::error!("error during read");

                }
            }
        }
    } else {
        defmt::warn!("Could not authenticate");
    }

    if mfrc522.hlta().is_err() {
        defmt::error!("Could not halt");
    }
    if mfrc522.stop_crypto1().is_err() {
        defmt::error!("Could not disable crypto1");
    }
}


#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}
