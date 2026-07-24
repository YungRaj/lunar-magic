use crate::Rgb8;

const SIDE: usize = 33;
const MOMENT_COUNT: usize = SIDE * SIDE * SIDE;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Moment {
    pub(super) weight: f64,
    pub(super) red: f64,
    pub(super) green: f64,
    pub(super) blue: f64,
    squared: f64,
}

impl Moment {
    fn add(self, other: Self) -> Self {
        Self {
            weight: self.weight + other.weight,
            red: self.red + other.red,
            green: self.green + other.green,
            blue: self.blue + other.blue,
            squared: self.squared + other.squared,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            weight: self.weight - other.weight,
            red: self.red - other.red,
            green: self.green - other.green,
            blue: self.blue - other.blue,
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
}

#[derive(Clone, Copy)]
pub(super) struct Split {
    pub(super) first: ColorBox,
    pub(super) second: ColorBox,
}

pub(super) fn build_moments(pixels: &[Rgb8]) -> Vec<Moment> {
    let mut moments = vec![Moment::default(); MOMENT_COUNT];
    for pixel in pixels {
        let red = usize::from(pixel.red >> 3) + 1;
        let green = usize::from(pixel.green >> 3) + 1;
        let blue = usize::from(pixel.blue >> 3) + 1;
        let value = &mut moments[index(red, green, blue)];
        value.weight += 1.0;
        value.red += f64::from(pixel.red);
        value.green += f64::from(pixel.green);
        value.blue += f64::from(pixel.blue);
        value.squared += f64::from(pixel.red) * f64::from(pixel.red)
            + f64::from(pixel.green) * f64::from(pixel.green)
            + f64::from(pixel.blue) * f64::from(pixel.blue);
    }

    for red in 1..SIDE {
        for green in 1..SIDE {
            for blue in 1..SIDE {
                let cumulative = moments[index(red, green, blue)]
                    .add(moments[index(red - 1, green, blue)])
                    .add(moments[index(red, green - 1, blue)])
                    .add(moments[index(red, green, blue - 1)])
                    .sub(moments[index(red - 1, green - 1, blue)])
                    .sub(moments[index(red - 1, green, blue - 1)])
                    .sub(moments[index(red, green - 1, blue - 1)])
                    .add(moments[index(red - 1, green - 1, blue - 1)]);
                moments[index(red, green, blue)] = cumulative;
            }
        }
    }
    moments
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

pub(super) fn variance(moments: &[Moment], color_box: ColorBox) -> f64 {
    let moment = volume(moments, color_box);
    if moment.weight == 0.0 {
        0.0
    } else {
        moment.squared
            - (moment.red * moment.red + moment.green * moment.green + moment.blue * moment.blue)
                / moment.weight
    }
}

pub(super) fn best_split(moments: &[Moment], color_box: ColorBox) -> Option<Split> {
    let mut best: Option<(f64, Split)> = None;
    for axis in [Axis::Red, Axis::Green, Axis::Blue] {
        let (low, high) = match axis {
            Axis::Red => (color_box.red_low, color_box.red_high),
            Axis::Green => (color_box.green_low, color_box.green_high),
            Axis::Blue => (color_box.blue_low, color_box.blue_high),
        };
        for cut in low + 1..high {
            let split = split_box(color_box, axis, cut);
            let first = volume(moments, split.first);
            let second = volume(moments, split.second);
            if first.weight == 0.0 || second.weight == 0.0 {
                continue;
            }
            let score = mean_square(first) + mean_square(second);
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

fn mean_square(moment: Moment) -> f64 {
    (moment.red * moment.red + moment.green * moment.green + moment.blue * moment.blue)
        / moment.weight
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
