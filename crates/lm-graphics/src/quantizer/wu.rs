use crate::Rgb8;

const SIDE: usize = 33;
const MOMENT_COUNT: usize = SIDE * SIDE * SIDE;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Moment {
    pub(super) weight: i32,
    pub(super) red: i32,
    pub(super) green: i32,
    pub(super) blue: i32,
    squared: f32,
}

impl Moment {
    fn add(self, other: Self) -> Self {
        Self {
            weight: self.weight.wrapping_add(other.weight),
            red: self.red.wrapping_add(other.red),
            green: self.green.wrapping_add(other.green),
            blue: self.blue.wrapping_add(other.blue),
            squared: self.squared + other.squared,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            weight: self.weight.wrapping_sub(other.weight),
            red: self.red.wrapping_sub(other.red),
            green: self.green.wrapping_sub(other.green),
            blue: self.blue.wrapping_sub(other.blue),
            squared: self.squared - other.squared,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ColorBox {
    red_low: usize,
    red_high: usize,
    green_low: usize,
    green_high: usize,
    blue_low: usize,
    blue_high: usize,
}

impl ColorBox {
    pub(super) const fn whole() -> Self {
        Self {
            red_low: 0,
            red_high: 32,
            green_low: 0,
            green_high: 32,
            blue_low: 0,
            blue_high: 32,
        }
    }

    pub(super) const fn geometric_volume(self) -> usize {
        (self.red_high - self.red_low)
            * (self.green_high - self.green_low)
            * (self.blue_high - self.blue_low)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Split {
    pub(super) first: ColorBox,
    pub(super) second: ColorBox,
}

pub(super) fn build_moments(pixels: &[Rgb8]) -> Vec<Moment> {
    let mut moments = vec![Moment::default(); MOMENT_COUNT];
    for pixel in pixels {
        // Lunar Magic's bitmap loader first reduces each component to its rounded 5-bit SNES
        // value, retained in the high five bits of an RGB32 component.  The quantizer therefore
        // observes 0, 8, ... 248 rather than the bitmap's original byte values.
        let pixel = snes_lattice_rgb(*pixel);
        let red = usize::from(pixel.red >> 3) + 1;
        let green = usize::from(pixel.green >> 3) + 1;
        let blue = usize::from(pixel.blue >> 3) + 1;
        let value = &mut moments[index(red, green, blue)];
        value.weight = value.weight.wrapping_add(1);
        value.red = value.red.wrapping_add(i32::from(pixel.red));
        value.green = value.green.wrapping_add(i32::from(pixel.green));
        value.blue = value.blue.wrapping_add(i32::from(pixel.blue));
        value.squared += f32::from(pixel.red) * f32::from(pixel.red)
            + f32::from(pixel.green) * f32::from(pixel.green)
            + f32::from(pixel.blue) * f32::from(pixel.blue);
    }

    // Lunar Magic builds each red plane from running blue lines and green-plane areas. Besides
    // avoiding redundant inclusion/exclusion work, preserving this order is observable because
    // the squared moment is accumulated in single precision.
    for red in 1..SIDE {
        let mut area = [Moment::default(); SIDE];
        for green in 1..SIDE {
            let mut line = Moment::default();
            for blue in 1..SIDE {
                line = line.add(moments[index(red, green, blue)]);
                area[blue] = area[blue].add(line);
                moments[index(red, green, blue)] =
                    moments[index(red - 1, green, blue)].add(area[blue]);
            }
        }
    }
    moments
}

pub(super) const fn snes_lattice_rgb(pixel: Rgb8) -> Rgb8 {
    Rgb8 {
        red: snes_lattice_channel(pixel.red),
        green: snes_lattice_channel(pixel.green),
        blue: snes_lattice_channel(pixel.blue),
    }
}

const fn snes_lattice_channel(channel: u8) -> u8 {
    let truncated = channel & 0xf8;
    if channel & 4 != 0 && truncated < 0xf8 {
        truncated + 8
    } else {
        truncated
    }
}

pub(super) fn volume(moments: &[Moment], color_box: ColorBox) -> Moment {
    let corner = |red, green, blue| moments[index(red, green, blue)];
    corner(
        color_box.red_high,
        color_box.green_high,
        color_box.blue_high,
    )
    .sub(corner(
        color_box.red_low,
        color_box.green_high,
        color_box.blue_high,
    ))
    .sub(corner(
        color_box.red_high,
        color_box.green_low,
        color_box.blue_high,
    ))
    .sub(corner(
        color_box.red_high,
        color_box.green_high,
        color_box.blue_low,
    ))
    .add(corner(
        color_box.red_low,
        color_box.green_low,
        color_box.blue_high,
    ))
    .add(corner(
        color_box.red_low,
        color_box.green_high,
        color_box.blue_low,
    ))
    .add(corner(
        color_box.red_high,
        color_box.green_low,
        color_box.blue_low,
    ))
    .sub(corner(
        color_box.red_low,
        color_box.green_low,
        color_box.blue_low,
    ))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(super) fn variance(moments: &[Moment], color_box: ColorBox) -> f32 {
    let moment = volume(moments, color_box);
    if moment.weight == 0 {
        0.0
    } else {
        // The native routine first stores each integer component sum as binary32, then reloads
        // those three values and performs the products, sum, division, and subtraction in the x87
        // register stack before one final binary32 store.
        let red = f64::from(moment.red as f32);
        let green = f64::from(moment.green as f32);
        let blue = f64::from(moment.blue as f32);
        (f64::from(moment.squared)
            - (red * red + green * green + blue * blue) / f64::from(moment.weight)) as f32
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(super) fn best_split(moments: &[Moment], color_box: ColorBox) -> Option<Split> {
    let mut best: Option<(f32, Split)> = None;
    // The native RGB32 histogram is indexed in byte order: blue, green, red.  Equal split scores
    // therefore retain a blue-axis candidate before green and red candidates.
    for axis in [Axis::Blue, Axis::Green, Axis::Red] {
        let (low, high) = match axis {
            Axis::Red => (color_box.red_low, color_box.red_high),
            Axis::Green => (color_box.green_low, color_box.green_high),
            Axis::Blue => (color_box.blue_low, color_box.blue_high),
        };
        for cut in low + 1..high {
            let split = split_box(color_box, axis, cut);
            let first = volume(moments, split.first);
            let second = volume(moments, split.second);
            if first.weight == 0 || second.weight == 0 {
                continue;
            }
            // The x86 implementation computes the first quotient in the x87 register stack,
            // rounds that half to binary32, then adds the second extended-precision quotient and
            // rounds the combined score to binary32. This asymmetric rounding is observable for
            // close cuts in larger palettes.
            let first_score = mean_square_extended(first) as f32;
            let score = (f64::from(first_score) + mean_square_extended(second)) as f32;
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                best = Some((score, split));
            }
        }
    }
    best.map(|(_, split)| split)
}

fn index(red: usize, green: usize, blue: usize) -> usize {
    (red * SIDE + green) * SIDE + blue
}

#[allow(clippy::cast_precision_loss)]
fn mean_square_extended(moment: Moment) -> f64 {
    let red = f64::from(moment.red);
    let green = f64::from(moment.green);
    let blue = f64::from(moment.blue);
    (red * red + green * green + blue * blue) / f64::from(moment.weight)
}

#[derive(Clone, Copy)]
enum Axis {
    Red,
    Green,
    Blue,
}

const fn split_box(mut color_box: ColorBox, axis: Axis, cut: usize) -> Split {
    let mut second = color_box;
    match axis {
        Axis::Red => {
            color_box.red_high = cut;
            second.red_low = cut;
        }
        Axis::Green => {
            color_box.green_high = cut;
            second.green_low = cut;
        }
        Axis::Blue => {
            color_box.blue_high = cut;
            second.blue_low = cut;
        }
    }
    Split {
        first: color_box,
        second,
    }
}
