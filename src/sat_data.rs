use std::fs;
use std::io::{Cursor, Read};
use std::sync::mpsc;
use std::thread::spawn;

use anyhow::Error;
use bytes::Bytes;
use opencv::imgcodecs::IMREAD_GRAYSCALE;
use opencv::prelude::Mat;
use zip::ZipArchive;

#[derive(Clone)]
pub struct SatData {
    mat_array: Vec<Mat>,
}

unsafe impl Send for SatData{

}

impl SatData {
    /// Create a new SatData instance from the unzipped file location
    pub fn new(mut data: ZipArchive<Cursor<Bytes>>) -> anyhow::Result<SatData> {
        // list of file patterns to look for
        let file_patterns = ["_B01.jp2", "_B02.jp2", "_B03.jp2", "_B04.jp2", "_B05.jp2", "_B06.jp2", "_B07.jp2", "_B08.jp2", "_B09.jp2", "_B10.jp2", "_B11.jp2", "_B12.jp2"];

        let mut thread_array = Vec::with_capacity(file_patterns.len());
        let mut mat_array = Vec::with_capacity(file_patterns.len());

        for index in 0..data.len() {
            let mut file = data.by_index(index).unwrap();

            for x in file_patterns {
                if file.name().contains(x) && file.enclosed_name().unwrap().parent().unwrap().parent().unwrap().parent().unwrap().file_name().unwrap() == "GRANULE" {
                    let mut d = Vec::new();
                    file.read_to_end(&mut d).unwrap();

                    let (tx, rx) = mpsc::channel();

                    thread_array.push(rx);

                    spawn(move || {
                        let mat_data = Mat::from_slice(&d).unwrap();
                        tx.send(opencv::imgcodecs::imdecode(&mat_data, IMREAD_GRAYSCALE).unwrap()).unwrap();
                    });
                }
            }
        }

        for x in thread_array {
            mat_array.push(
                x.recv().unwrap()
            )
        }

        Ok(SatData {
            mat_array
        })
    }

    fn load_image(path: &str) -> Mat {
        opencv::imgcodecs::imread(path, IMREAD_GRAYSCALE).unwrap()
    }

    fn get_file_with_pattern(root_path: &str, pattern: &str) -> anyhow::Result<Mat> {
        let f = fs::read_dir(root_path).unwrap();

        for x in f {
            let x_unwrapped = x.unwrap();

            if x_unwrapped.file_name().to_str().unwrap().contains(pattern) {
                return Ok(SatData::load_image(x_unwrapped.path().to_str().unwrap()));
            }
        }

        Err(Error::msg("Unable to find image!"))
    }

    pub fn get_b1(&self) -> Mat {
        self.mat_array[0].clone()
    }
    pub fn get_b2(&self) -> Mat {
        self.mat_array[1].clone()
    }
    pub fn get_b3(&self) -> Mat {
        self.mat_array[2].clone()
    }
    pub fn get_b4(&self) -> Mat {
        self.mat_array[3].clone()
    }
    pub fn get_b5(&self) -> Mat {
        self.mat_array[4].clone()
    }
    pub fn get_b6(&self) -> Mat {
        self.mat_array[5].clone()
    }
    pub fn get_b7(&self) -> Mat {
        self.mat_array[6].clone()
    }
    pub fn get_b8(&self) -> Mat {
        self.mat_array[7].clone()
    }
    pub fn get_b9(&self) -> Mat {
        self.mat_array[8].clone()
    }
    pub fn get_b10(&self) -> Mat {
        self.mat_array[9].clone()
    }
    pub fn get_b11(&self) -> Mat {
        self.mat_array[10].clone()
    }
    pub fn get_b12(&self) -> Mat {
        self.mat_array[11].clone()
    }
}