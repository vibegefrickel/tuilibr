//! The primaries of an HDR source and of the targets, and the matrices between them.

use crate::pictures::decode::color::ColorTarget;

/// All but DCI-P3 have the white point D65.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Primaries {
    Bt709,
    DisplayP3,
    /// SMPTE RP 431-2: the P3 primaries with the DCI white point.
    DciP3,
    Bt2020,
}

impl Primaries {
    /// Other codes count as BT.2020, the primaries of BT.2100.
    pub(super) fn of_cicp(primaries: u8) -> Self {
        match primaries {
            1 => Self::Bt709,
            11 => Self::DciP3,
            12 => Self::DisplayP3,
            _ => Self::Bt2020,
        }
    }

    pub(super) fn of_target(target: ColorTarget) -> Self {
        match target {
            ColorTarget::DisplayP3 => Self::DisplayP3,
            ColorTarget::Srgb => Self::Bt709,
        }
    }

    fn chromaticities(self) -> [[f64; 2]; 3] {
        match self {
            Self::Bt709 => [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]],
            Self::DisplayP3 | Self::DciP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
            Self::Bt2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]],
        }
    }

    fn white(self) -> [f64; 2] {
        match self {
            Self::DciP3 => [0.314, 0.351],
            _ => D65,
        }
    }
}

const D65: [f64; 2] = [0.3127, 0.3290];

type Matrix = [[f64; 3]; 3];

pub(super) fn apply(matrix: &Matrix, vector: [f64; 3]) -> [f64; 3] {
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

/// The matrices of real primaries are never singular.
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

/// CIE XYZ of a chromaticity at Y = 1.
fn xyz([x, y]: [f64; 2]) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// Linear RGB to CIE XYZ, with white at Y = 1.
pub(super) fn rgb_to_xyz(primaries: Primaries) -> Matrix {
    let [red, green, blue] = primaries.chromaticities().map(xyz);
    let columns = [0, 1, 2].map(|row| [red[row], green[row], blue[row]]);
    let scale = apply(&invert(&columns), xyz(primaries.white()));
    columns.map(|row| [0, 1, 2].map(|column| row[column] * scale[column]))
}

/// The Bradford cone response matrix of `player/tone_map.rs`.
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

/// Linear RGB of `source` to linear RGB of `target`, white to white, as in the player cubes.
pub(super) fn conversion(source: Primaries, target: Primaries) -> Matrix {
    let adapted = multiply(
        &adaptation(source.white(), target.white()),
        &rgb_to_xyz(source),
    );
    multiply(&invert(&rgb_to_xyz(target)), &adapted)
}
