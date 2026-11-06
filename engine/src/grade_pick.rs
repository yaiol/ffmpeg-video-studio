//! What the colour pickers of the work view do with a colour taken from the picture (red, green, blue, 0 to 1): the three of them are the
//! eyedroppers of the grading panel - a black point (the lift), a white point (the gain) and a white balance (temperature and tint). They
//! invert the expressions of the `colour` effect of the catalogue, so a value that is neutral for the effect is neutral here.

use crate::ffmpeg;
use std::{path::Path, process::Stdio};

/// What a tint of 1 adds to the red and blue gain and takes from the green gain (the same factors as the effect's expressions).
const TINT_RB: f64 = 0.001;
const TINT_G: f64 = -0.002;

/// The red, green and blue lift that bring the picked colour to black. `master` is the lift every channel already has; the result is what the
/// three channels carry in addition, kept inside the control's range.
pub fn black_lift(picked: [f64; 3], master: f64) -> [f64; 3] {
    picked.map(|p| {
        let p = p.clamp(0.0, 0.98);
        (-p / (1.0 - p) - master).clamp(-1.0, 1.0)
    })
}

/// The red, green and blue gain that bring the picked colour to white, with the master gain and the tint the effect already has.
pub fn white_gain(picked: [f64; 3], master: f64, tint: f64) -> [f64; 3] {
    let tinted = [1.0 + TINT_RB * tint, 1.0 + TINT_G * tint, 1.0 + TINT_RB * tint];
    let mut out = [1.0; 3];
    for i in 0..3 {
        out[i] = (1.0 / (picked[i].max(0.02) * master.max(0.01) * tinted[i])).clamp(0.0, 4.0);
    }
    out
}

/// The colour a solid patch comes out as after the effect's temperature step (`temp` as the control has it: the filter is given 6500 K
/// times two to the minus temp over a hundred), through ffmpeg itself, because the filter's curve is ffmpeg's.
fn warmed(ffmpeg: &Path, colour: [f64; 3], temp: f64) -> Result<[f64; 3], String> {
    let hex = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let source = format!("color=c=0x{:02x}{:02x}{:02x}:s=2x2:r=1:d=1", hex(colour[0]), hex(colour[1]), hex(colour[2]));
    let filter = format!("colortemperature=temperature={:.3},format=rgb24", 6500.0 * (-temp / 100.0).exp2());
    let out = ffmpeg::command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-f", "lavfi", "-i", &source, "-vf", &filter, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    if out.stdout.len() < 3 {
        return Err(String::from_utf8_lossy(&out.stderr).lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no picture").to_owned());
    }
    Ok([f64::from(out.stdout[0]) / 255.0, f64::from(out.stdout[1]) / 255.0, f64::from(out.stdout[2]) / 255.0])
}

/// The temperature and the tint (both whole numbers, -100 to 100) that make the picked colour grey. The temperature is found by halving the
/// range until red and blue come out equal; the tint then evens the green against them.
pub fn white_balance(ffmpeg: &Path, picked: [f64; 3]) -> Result<(f64, f64), String> {
    let ratio = |t: f64| warmed(ffmpeg, picked, t).map(|c| (c[0] + 0.004) / (c[2] + 0.004));
    let (mut low, mut high) = (-100.0_f64, 100.0_f64);
    // a warmer picture (a higher temperature) has more red against blue
    let temp = if ratio(low)? >= 1.0 {
        low
    } else if ratio(high)? <= 1.0 {
        high
    } else {
        for _ in 0..8 {
            let mid = (low + high) / 2.0;
            if ratio(mid)? < 1.0 { low = mid } else { high = mid }
        }
        ((low + high) / 2.0).round()
    };
    let c = warmed(ffmpeg, picked, temp)?;
    let rb = (c[0] + c[2]) / 2.0;
    // green * (1 + TINT_G * tint) = rb * (1 + TINT_RB * tint)
    let tint = if c[1] <= 0.0 && rb <= 0.0 { 0.0 } else { ((c[1] - rb) / (-TINT_G * c[1] + TINT_RB * rb)).clamp(-100.0, 100.0) };
    Ok((temp, tint.round()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ffmpeg_for_tests() -> Option<PathBuf> {
        let name = ffmpeg::executable_name();
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
    }

    #[test]
    fn a_picked_black_gets_a_lift_that_takes_it_to_zero_and_a_pure_black_gets_none() {
        let lift = black_lift([0.2, 0.1, 0.0], 0.0);
        for (l, p) in lift.iter().zip([0.2, 0.1, 0.0]) {
            assert!((p + l * (1.0 - p)).abs() < 1e-9, "{p} lifted by {l}");
        }
        assert_eq!(lift[2], 0.0);
        // the master lift already does part of it
        assert!((black_lift([0.2, 0.2, 0.2], -0.1)[0] - (-0.25 + 0.1)).abs() < 1e-9);
    }

    #[test]
    fn a_picked_white_gets_a_gain_that_takes_it_to_one_inside_the_range() {
        let g = white_gain([0.5, 0.8, 1.0], 1.0, 0.0);
        assert!((g[0] - 2.0).abs() < 1e-9 && (g[1] - 1.25).abs() < 1e-9 && (g[2] - 1.0).abs() < 1e-9);
        assert_eq!(white_gain([0.01, 0.5, 0.5], 1.0, 0.0)[0], 4.0, "a nearly black pick is held at the top of the range");
        // the master gain and the tint are part of the product
        assert!((white_gain([0.5, 0.5, 0.5], 2.0, 0.0)[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_white_balance_of_a_warm_grey_cools_it_and_of_a_grey_leaves_it() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let (temp, tint) = white_balance(&ff, [0.6, 0.55, 0.5]).unwrap();
        assert!(temp < 0.0, "a warm patch needs a cooler picture, got {temp}");
        let after = warmed(&ff, [0.6, 0.55, 0.5], temp).unwrap();
        assert!((after[0] - after[2]).abs() < 0.04, "red and blue still differ: {after:?}");
        assert!(tint.abs() <= 100.0);
        let (t, _) = white_balance(&ff, [0.5, 0.5, 0.5]).unwrap();
        assert!(t.abs() <= 12.0, "a grey needs about nothing, got {t}");
    }
}
