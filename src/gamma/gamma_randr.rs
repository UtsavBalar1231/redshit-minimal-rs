use crate::colorramp;
use crate::transition;
use xcb::{randr, x, Xid};

use super::GammaMethod;
use super::Result;
use std::error::Error;
use std::fmt;

const RANDR_MAJOR_VERSION: u32 = 1;
const RANDR_MINOR_VERSION: u32 = 3;

/// Wrapper for XCB and RandR errors
pub enum RandrError {
    Generic(xcb::Error),
    Conn(xcb::ConnError),
    UnsupportedVersion(u32, u32),
}

impl RandrError {
    fn generic(e: xcb::Error) -> Box<dyn Error> {
        Box::new(RandrError::Generic(e)) as Box<dyn Error>
    }
}

impl RandrError {
    fn conn(e: xcb::ConnError) -> Box<dyn Error> {
        Box::new(RandrError::Conn(e)) as Box<dyn Error>
    }

    fn unsupported_version(major: u32, minor: u32) -> Box<dyn Error> {
        Box::new(RandrError::UnsupportedVersion(major, minor)) as Box<dyn Error>
    }
}

impl fmt::Display for RandrError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for RandrError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RandrError::*;
        match *self {
            Generic(ref e) => write!(f, "randr error: {}", e),
            Conn(xcb::ConnError::Connection) => write!(
                f,
                "xcb connection errors because of socket, pipe or other stream errors"
            ),
            Conn(ref c) => write!(f, "{c:?}"),
            UnsupportedVersion(major, minor) => {
                write!(f, "Unsupported RandR version ({major}.{minor})")
            }
        }
    }
}

impl Error for RandrError {
    fn description(&self) -> &str {
        "RandR error"
    }
}

struct Crtc {
    /// The id of CRTC (gotten from XCB)
    id: u32,

    /// The ramp size.
    ramp_size: u16,

    /// The initial gamma ramp values - used for restore
    saved_ramps: (Vec<u16>, Vec<u16>, Vec<u16>),

    /// A scratchpad for color computation - it saves the cost of
    /// allocating three new arrays whenever set_temperature() is
    /// called.
    scratch: (Vec<u16>, Vec<u16>, Vec<u16>),
}

/// Wrapping struct for RandR state
pub struct RandrState {
    conn: xcb::Connection,
    window_dummy: x::Window,
    crtcs: Vec<Crtc>,
}

impl RandrState {
    fn init() -> Result<RandrState> {
        let (conn, screen_num) = xcb::Connection::connect(None).map_err(RandrError::conn)?;

        query_version(&conn)?;

        let window_dummy = {
            let setup = conn.get_setup();
            let screen = setup.roots().nth(screen_num as usize).unwrap();
            let window_dummy = conn.generate_id();

            conn.send_request(&x::CreateWindow {
                depth: x::COPY_FROM_PARENT as u8,
                wid: window_dummy,
                parent: screen.root(),
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                border_width: 0,
                class: x::WindowClass::InputOutput,
                visual: screen.root_visual(),
                value_list: &[],
            });

            conn.flush()?;
            window_dummy
        };

        Ok(RandrState {
            conn,
            window_dummy,
            crtcs: vec![],
        })
    }

    // Set the temperature for the indicated CRTC
    fn set_crtc_temperatures(&mut self, setting: &transition::ColorSetting) -> Result<()> {
        for crtc in self.crtcs.iter_mut() {
            let (ref mut r, ref mut g, ref mut b) = crtc.scratch;

            let u16_max1 = u16::max_value() as f64 + 1.0;
            let ramp_size = crtc.ramp_size as f64;
            for i in 0..r.len() {
                let v = ((i as f64 / ramp_size) * u16_max1) as u16;
                r[i] = v;
                g[i] = v;
                b[i] = v;
            }

            // Compute new gamma ramps
            colorramp::fill(
                &mut r[..],
                &mut g[..],
                &mut b[..],
                setting,
                crtc.ramp_size as usize,
            );

            // Set the gamma ramp
            unsafe {
                self.conn.send_request(&randr::SetCrtcGamma {
                    crtc: xcb::XidNew::new(crtc.id),
                    red: &r[..],
                    green: &g[..],
                    blue: &b[..],
                });
            }

            // Save the new gamma ramps
            crtc.saved_ramps = (r.clone(), g.clone(), b.clone());

            self.conn.flush()?;
        }
        Ok(())
    }
}

fn query_version(conn: &xcb::Connection) -> Result<()> {
    let req = randr::QueryVersion {
        major_version: RANDR_MAJOR_VERSION,
        minor_version: RANDR_MINOR_VERSION,
    };

    // send and check for errors
    let cookie = conn.send_request(&req);

    let reply = conn.wait_for_reply(cookie).map_err(RandrError::generic)?;

    if reply.major_version() < RANDR_MAJOR_VERSION
        || (reply.major_version() == RANDR_MAJOR_VERSION
            && reply.minor_version() < RANDR_MINOR_VERSION)
    {
        return Err(RandrError::unsupported_version(
            reply.major_version(),
            reply.minor_version(),
        ));
    }

    conn.flush()?;

    Ok(())
}

impl GammaMethod for RandrState {
    //
    // Restore saved gamma ramps
    //
    fn restore(&self) -> Result<()> {
        for crtc in self.crtcs.iter() {
            unsafe {
                self.conn.send_request(&randr::SetCrtcGamma {
                    crtc: xcb::XidNew::new(crtc.id),
                    red: &crtc.saved_ramps.0[..],
                    green: &crtc.saved_ramps.1[..],
                    blue: &crtc.saved_ramps.2[..],
                });
            }

            self.conn.flush()?;
        }
        Ok(())
    }

    fn set_temperature(&mut self, setting: &transition::ColorSetting) -> Result<()> {
        self.set_crtc_temperatures(setting)
    }

    /// Find initial information on all the CRTCs
    fn start(&mut self) -> Result<()> {
        // Get list of CRTCs for the screen

        let req = self.conn.send_request(&randr::GetScreenResources {
            window: self.window_dummy,
        });

        let reply = self.conn.wait_for_reply(req).map_err(RandrError::generic)?;

        let crtcs = reply.crtcs();

        self.crtcs = Vec::with_capacity(crtcs.len() as usize);

        // Save size and gamma ramps of all CRTCs
        for crtc in crtcs {
            let req = self
                .conn
                .send_request(&randr::GetCrtcGammaSize { crtc: *crtc });

            let reply = self.conn.wait_for_reply(req).map_err(RandrError::generic)?;

            let ramp_size = reply.size();

            let req = self.conn.send_request(&randr::GetCrtcGamma { crtc: *crtc });

            let reply = self.conn.wait_for_reply(req).map_err(RandrError::generic)?;

            let red = reply.red().to_vec();
            let green = reply.green().to_vec();
            let blue = reply.blue().to_vec();

            self.crtcs.push(Crtc {
                id: crtc.resource_id(),
                ramp_size,
                saved_ramps: (red.clone(), green.clone(), blue.clone()),
                scratch: (red.clone(), green.clone(), blue.clone()),
            });
        }
        Ok(())
    }
}

/// The init function
pub fn init() -> Result<Box<dyn GammaMethod>> {
    RandrState::init().map(|r| Box::new(r) as Box<dyn GammaMethod>)
}
