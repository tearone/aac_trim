pub struct Waveform;

impl Waveform {
    pub fn avgfilter(vec: &[i16]) -> f64 {
        let mut sum = 0_f64;
        let mut c = 0;

        for i in vec {
            if i < &0 {
                continue;
            }

            sum += *i as f64;
            c += 1;
        }

        sum / c as f64
    }

    pub fn avgmax(vec: &[f64]) -> (f64, f64) {
        let mut sum = 0_f64;
        let mut c = 0;
        let mut max = 0_f64;

        for i in vec {
            if i > &max {
                max = *i;
            }
            sum += *i as f64;
            c += 1;
        }

        (sum / c as f64, max)
    }

    pub fn balance(vec: &[f64], max: f64) -> Vec<u8> {
        vec.iter()
            .map(|i| (i / max * 256_f64).round() as u8)
            .collect::<Vec<u8>>()
    }

    // pub fn rms_range(vec: &[f64]) -> Option<f64> {
    //     let mut sq = 0_f64;

    //     for i in vec {
    //         sq += i * i;
    //     }

    //     let mean: f64 = sq as f64 / vec.len() as f64;

    //     let root = mean.sqrt();

    //     if !root.is_nan() {
    //         Some(root)
    //     } else {
    //         None
    //     }
    // }

    pub fn peak_range(vec: &[f64]) -> Option<f64> {
        let mut max = 0_f64;

        for i in vec {
            if i > &max {
                max = *i;
            }
        }

        Some(max)
    }
}
