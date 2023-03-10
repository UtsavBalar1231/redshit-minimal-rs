#[cfg(feature = "randr")]
mod gamma_randr;

use super::Result;
use crate::transition;

use std::collections::HashMap;
use std::error::Error;

type GammaInit = fn() -> Result<Box<dyn GammaMethod>>;

lazy_static! {
    static ref SUPPORTED_GAMMA_METHODS: HashMap<&'static str, GammaInit> = {
        let mut m: HashMap<&'static str, GammaInit> = HashMap::with_capacity(4);

        #[cfg(feature = "randr")]
        m.insert("randr", gamma_randr::init);

        m.insert("dummy", init_dummy);
        m
    };
}

/// Any gamma method provider should implement this trait
///
pub trait GammaMethod {
    /// Initialization method
    ///
    /// Called before set_temperature()
    fn start(&mut self) -> Result<()>;

    /// Use the given color setting to adjust the screen temperature
    ///
    /// When running continually, this method is invoked
    /// repeatedly. In oneshot mode, this method is invoked once.
    fn set_temperature(&mut self, setting: &transition::ColorSetting) -> Result<()>;

    /// The restore method is called when Redshift exits from
    /// running in continual mode.
    fn restore(&self) -> Result<()>;
}

fn init_dummy() -> Result<Box<dyn GammaMethod>> {
    Ok(Box::new(DummyMethod) as Box<dyn GammaMethod>)
}

/// Initialise the gamma adjustment method
///
/// If a specific method is requsted (ie method_name is `Some(..)`)
/// then it is assumed that the method exists and we can call its
/// initialisation function. If a requested method does not exist,
/// this function panics.
///
/// If `method_name` is `None` then all available methods (except for
/// the dummy) are tried in turn until one successfully starts - and
/// then that method is used.
pub fn init_gamma_method(method_name: Option<&str>) -> Result<Box<dyn GammaMethod>> {
    match method_name {
        Some(m) => SUPPORTED_GAMMA_METHODS[m](),
        None => {
            // Loop over each method and try their init function
            // (skipping the dummy)
            SUPPORTED_GAMMA_METHODS
                .iter()
                .filter_map(|(name, method_init)| {
                    if &name[..] == "dummy" {
                        None
                    } else {
                        method_init().ok()
                    }
                })
                .take(1)
                .next()
                .ok_or_else(|| Box::<dyn Error>::from("No gamma adjustment method available"))
        }
    }
}

pub struct DummyMethod;
impl GammaMethod for DummyMethod {
    fn restore(&self) -> Result<()> {
        Ok(())
    }

    fn set_temperature(&mut self, setting: &transition::ColorSetting) -> Result<()> {
        println!("Temperature: {}", setting.temp);
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        println!(
            "WARNING: Using dummy gamma method! Display will not affected by this gamma method."
        );
        Ok(())
    }
}
