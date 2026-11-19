//! The color math of the cubes: the primaries matrices, the transfer curves and the tone map.

use super::{HDR_PEAK_NITS, Hdr, Primaries, SDR_WHITE_NITS};

type Matrix = [[f64; 3]; 3];

fn apply(matrix: &Matrix, vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

fn multiply(left: &Matrix, right: &Matrix) -> Matrix {
    left.map(|row| {
        [0, 1, 2].map(|column| {
            row[0] * right[0][column] + row[1] * right[1][column] + row[2] * right[2][column]
        })
    })
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

/// The columns are the cross products of the rows over the determinant. The matrices of real
/// primaries are never singular.
fn invert(matrix: &Matrix) -> Matrix {
    let [first, second, third] = *matrix;
    let columns = [
        cross(second, third),
        cross(third, first),
        cross(first, second),
    ];
    let determinant =
        first[0] * columns[0][0] + first[1] * columns[0][1] + first[2] * columns[0][2];
    [0, 1, 2].map(|row| columns.map(|column| column[row] / determinant))
}

/// At Y = 1.
fn xyz([x, y]: [f64; 2]) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// From linear RGB to CIE XYZ, with the white at Y = 1.
pub(super) fn rgb_to_xyz(primaries: Primaries) -> Matrix {
    let [red, green, blue] = primaries.chromaticities().map(xyz);
    let columns = [0, 1, 2].map(|row| [red[row], green[row], blue[row]]);
    let scale = apply(&invert(&columns), xyz(primaries.white()));
    columns.map(|row| [0, 1, 2].map(|column| row[column] * scale[column]))
}

/// The Bradford cone response matrix, as in the ICC profiles that `decode::color` reads.
const BRADFORD: Matrix = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

fn adaptation(source: [f64; 2], target: [f64; 2]) -> Matrix {
    let (from, to) = (apply(&BRADFORD, xyz(source)), apply(&BRADFORD, xyz(target)));
    let gains = [0, 1, 2].map(|row| {
        let mut line = [0.0; 3];
        line[row] = to[row] / from[row];
        line
    });
    multiply(&invert(&BRADFORD), &multiply(&gains, &BRADFORD))
}

/// Linear RGB of `source` to linear RGB of `target`, white to white.
pub(super) fn conversion(source: Primaries, target: Primaries) -> Matrix {
    let adapted = multiply(
        &adaptation(source.white(), target.white()),
        &rgb_to_xyz(source),
    );
    multiply(&invert(&rgb_to_xyz(target)), &adapted)
}

fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// A color outside the target clips per channel, as in `decode::color`.
pub(super) fn convert_sdr(matrix: &Matrix, signal: [f64; 3]) -> [f64; 3] {
    apply(matrix, signal.map(srgb_to_linear)).map(|value| linear_to_srgb(value.clamp(0.0, 1.0)))
}

/// Display light in nits for one BT.2020 HDR signal value per channel.
fn hdr_nits(hdr: Hdr, signal: [f64; 3]) -> [f64; 3] {
    match hdr {
        Hdr::Hlg => {
            // The inverse OETF to scene light, then the OOTF of a 1000 nit display (gamma 1.2).
            let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_73);
            let scene = signal.map(|value| {
                if value <= 0.5 {
                    value * value / 3.0
                } else {
                    (((value - c) / a).exp() + b) / 12.0
                }
            });
            let luma = 0.2627 * scene[0] + 0.6780 * scene[1] + 0.0593 * scene[2];
            let gain = HDR_PEAK_NITS * luma.max(0.0).powf(0.2);
            scene.map(|value| value * gain)
        }
        Hdr::Pq => {
            let (m1, m2) = (0.159_301_757_812_5, 78.843_75);
            let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.6875);
            signal.map(|value| {
                let power = value.powf(1.0 / m2);
                10_000.0 * ((power - c1).max(0.0) / (c2 - c3 * power)).powf(1.0 / m1)
            })
        }
    }
}

/// One SDR output value per channel (gamma 2.4) in the target primaries. `luma` is the Y row
/// of the target.
///
/// The highlights roll off with an extended Reinhard curve on the luminance, which reaches
/// white at `peak` nits, and the hue stays. A color outside the target moves toward its gray of
/// the same luminance.
pub(super) fn tone_map(
    hdr: Hdr,
    peak: f64,
    matrix: &Matrix,
    luma: [f64; 3],
    signal: [f64; 3],
) -> [f64; 3] {
    let nits = hdr_nits(hdr, signal);
    let mut linear = apply(matrix, nits).map(|value| value / SDR_WHITE_NITS);
    let luminance = luma[0] * linear[0] + luma[1] * linear[1] + luma[2] * linear[2];
    if luminance > 0.0 {
        let peak = peak / SDR_WHITE_NITS;
        let scale = (1.0 + luminance / (peak * peak)) / (1.0 + luminance);
        linear = linear.map(|value| value * scale);
        let mapped = luminance * scale;
        let low = linear[0].min(linear[1]).min(linear[2]);
        if low < 0.0 {
            let toward_gray = -low / (mapped - low);
            linear = linear.map(|value| value + (mapped - value) * toward_gray);
        }
    }
    // A channel above white (a saturated highlight, or PQ above the peak) scales the whole
    // color down. The clip keeps the hue.
    let high = linear[0].max(linear[1]).max(linear[2]);
    if high > 1.0 {
        linear = linear.map(|value| value / high);
    }
    linear.map(|value| value.clamp(0.0, 1.0).powf(1.0 / 2.4))
}

#[cfg(test)]
mod tests {
    use super::{Primaries, conversion, multiply, rgb_to_xyz};

    #[test]
    fn primaries_give_known_matrices() {
        let to_bt709 = conversion(Primaries::Bt2020, Primaries::Bt709);
        let expected = [
            [1.6605, -0.5876, -0.0728],
            [-0.1246, 1.1329, -0.0083],
            [-0.0182, -0.1006, 1.1187],
        ];
        for (row, expected) in to_bt709.iter().zip(expected) {
            for (value, expected) in row.iter().zip(expected) {
                assert!((value - expected).abs() < 1e-4, "{to_bt709:?}");
            }
        }
        let luma = rgb_to_xyz(Primaries::Bt709)[1];
        assert!((luma[0] - 0.2126).abs() < 1e-4 && (luma[1] - 0.7152).abs() < 1e-4);
        let round_trip = multiply(
            &conversion(Primaries::DisplayP3, Primaries::Bt709),
            &conversion(Primaries::Bt709, Primaries::DisplayP3),
        );
        for (row, line) in round_trip.iter().enumerate() {
            for (column, value) in line.iter().enumerate() {
                let identity = if row == column { 1.0 } else { 0.0 };
                assert!((value - identity).abs() < 1e-9);
            }
        }
    }
}
