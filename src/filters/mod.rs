use opencv::core::no_array;
use opencv::prelude::{Mat, MatTrait, MatTraitConst};
use opencv::types::VectorOfMat;

use crate::sat_data::SatData;

//. This will reduce the number pixels in an image by some ratio
fn down_sample_u8(m: Mat, ratio: usize) -> Mat {
    let mut new_m = unsafe {
        Mat::new_rows_cols(m.size().unwrap().width / ratio as i32, m.size().unwrap().height / ratio as i32, m.typ()).unwrap()
    };

    // mark areas less then 0.5 as water and above as land
    for x in (0..m.size().unwrap().width).step_by(ratio) {
        for y in (0..m.size().unwrap().height).step_by(ratio) {
            *new_m.at_2d_mut::<u8>(x / ratio as i32, y / ratio as i32).unwrap() = *m.at_2d::<u8>(x, y).unwrap();
        }
    }

    new_m
}

/// Basic combination of colors in red, green, and blue for the respective bands
fn simple_composite(r: Mat, g: Mat, b: Mat) -> Mat {
    // Create a new channel
    let mut channels = VectorOfMat::with_capacity(3);

    // add channels for color
    channels.push(b); // blue
    channels.push(g); // green
    channels.push(r); // red

    // make a new image
    let mut new_image = Mat::default();

    // merge color bands
    opencv::core::merge(&channels as _, &mut new_image).unwrap();

    // return
    new_image
}

// this will highlight land in green and water in blue (helps with land and water mapping)
pub fn ndwi(data: SatData) -> Mat {
    let green = data.get_b3();
    let nir = data.get_b8();

    let mut green_f32 = Mat::default();
    let mut nir_f32 = Mat::default();

    let mut top = Mat::default();
    let mut bottom = Mat::default();
    let mut ndwi_water = Mat::default();
    let mut ndwi_water_u8 = Mat::default();

    let mut empty = Mat::default();

    green.convert_to(&mut green_f32, opencv::core::CV_32F, 1., 0.).unwrap();
    nir.convert_to(&mut nir_f32, opencv::core::CV_32F, 1., 0.).unwrap();

    let mut ndwi_land = nir_f32.clone();
    let mut ndwi_land_u8 = Mat::default();

    // make a all zero Mat
    opencv::core::multiply(&nir, &0.0, &mut empty, 1.0, -1).unwrap();

    // preform ndwi first step
    opencv::core::subtract(&(green_f32), &(nir_f32), &mut top, &no_array(), -1).unwrap();

    // preform ndwi second step
    opencv::core::add(&green_f32, &nir_f32, &mut bottom, &no_array(), -1).unwrap();

    // finish ndwi
    opencv::core::divide2(&top, &bottom, &mut ndwi_water, 1.0, -1).unwrap();

    // mark areas less then 0.5 as water and above as land
    for x in 0..nir.size().unwrap().width {
        for y in 0..nir.size().unwrap().height {
            *ndwi_land.at_2d_mut::<f32>(x, y).unwrap() = (1.0 - ndwi_water.at_2d::<f32>(x, y).unwrap()).powi(3);
        }
    }

    // convert back to u8
    ndwi_water.convert_to(&mut ndwi_water_u8, opencv::core::CV_8UC1, 255.0, 0.0).unwrap();
    ndwi_land.convert_to(&mut ndwi_land_u8, opencv::core::CV_8UC1, 100.0, 0.0).unwrap();

    simple_composite(empty, ndwi_land_u8, ndwi_water_u8)
}

/// This combines sentinel 2 bands to make "true color" image or what it would look like to a human
/// if they were in space
pub fn true_color(data: SatData) -> Mat {
    simple_composite(
        data.get_b4(),
        data.get_b3(),
        data.get_b2(),
    )
}

/// This is almost the same as true color expect that red is a near infrared frequency. This makes
/// it easy to spot planet density
pub fn false_color(data: SatData) -> Mat {
    simple_composite(
        data.get_b8(),
        data.get_b3(),
        data.get_b2(),
    )
}

/// This highlights the density of water in green. The more green an area is, the more moister they
/// have. This will highlight dense vegetation areas and the presence of ice or rain in clouds. This
/// will also can highlight areas lacking in water like burn areas.
pub fn swir(data: SatData) -> Mat {

    // get bands
    let b4 = data.get_b4();
    let b8 = data.get_b8();
    let b12 = data.get_b12();

    let b4_down_sampled = down_sample_u8(b4, 2);
    let b8_down_sampled = down_sample_u8(b8, 2);

    simple_composite(
        b4_down_sampled,
        b8_down_sampled,
        b12,
    )
}
