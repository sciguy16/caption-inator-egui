use crate::Result;
use egui::Pos2;
use regex::Regex;
use std::process::Command;

#[derive(Copy, Clone, Debug, Default)]
pub struct MonitorPositions {
    pub internal: Pos2,
    pub external: Pos2,
}

pub fn monitor_positions() -> MonitorPositions {
    let displays = listmonitors()
        .inspect_err(|err| warn!("{err:?}"))
        .unwrap_or_default();
    info!("Discovered displays: {displays:?}");

    let Some(internal_display) = displays
        .iter()
        .find(|display| display.name.contains("LVDS"))
    else {
        return Default::default();
    };
    let Some(hdmi_display) = displays
        .iter()
        .find(|display| display.name.contains("HDMI"))
    else {
        return Default::default();
    };

    let (x, y) = internal_display.position;
    let internal = (x as f32, y as f32).into();
    let (x, y) = hdmi_display.position;
    let external = (x as f32, y as f32).into();

    dbg!(MonitorPositions { internal, external })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Display {
    size: (u32, u32),
    position: (u32, u32),
    name: String,
}

fn listmonitors() -> Result<Vec<Display>> {
    let output = Command::new("xrandr").arg("--listmonitors").output()?;
    if !output.status.success() {
        warn!("Call to xrandr failed with status {}!", output.status);
        warn!("stdout:  {}", String::from_utf8_lossy(&output.stdout));
        warn!("stderr:  {}", String::from_utf8_lossy(&output.stderr));
    }
    let output = String::from_utf8(output.stdout)?;

    parse_listmonitors(&output)
}

fn parse_listmonitors(output: &str) -> Result<Vec<Display>> {
    let mut ret = Vec::new();
    let regex = Regex::new(
        r"\s*\d+:\s[^\s]+\s([\d]+)/\d+x(\d+)/\d+\+(\d+)\+(\d+)\s+([A-Z0-9-]+)$",
    )?;

    for line in output.lines() {
        if line.starts_with("Monitors") {
            continue;
        }

        let Some(captures) = regex.captures(line) else {
            continue;
        };
        let (_, [width, height, offset_x, offset_y, name]) = captures.extract();

        ret.push(Display {
            size: (width.parse()?, height.parse()?),
            position: (offset_x.parse()?, offset_y.parse()?),
            name: name.into(),
        });
    }

    Ok(ret)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse() {
        const OUTPUT: &str = "\
Monitors: 2
 0: +*LVDS-1 1600/309x900/174+0+0  LVDS-1
 1: +HDMI-1 1920/476x1080/267+1600+0  HDMI-1
";

        assert_eq!(
            parse_listmonitors(OUTPUT).unwrap(),
            [
                Display {
                    size: (1600, 900),
                    position: (0, 0),
                    name: "LVDS-1".into()
                },
                Display {
                    size: (1920, 1080),
                    position: (1600, 0),
                    name: "HDMI-1".into()
                }
            ]
        );
    }
}
