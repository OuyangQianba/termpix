extern crate docopt;
extern crate image;
extern crate resvg;
extern crate usvg;
#[macro_use]
extern crate serde_derive;
extern crate terminal_size;
extern crate termpix;

use std::io::Write;

use docopt::Docopt;
use image::GenericImageView;
use image::*;
use terminal_size::{terminal_size, Height, Width};

use std::cmp::min;

const USAGE: &'static str = "
    termpix : display image from <file> in an ANSI terminal

    Usage:
      termpix <file> [--width <width>] [--height <height>] [--max-width <max-width>] [--max-height <max-height>] [--true-color|--true-colour] [--filter <nearest|triangle|catmullrom|gaussian|lanczos3>]

      By default it will use as much of the current terminal window as possible, while maintaining the aspect 
      ratio of the input image. This can be overridden as follows.

    Options:
      --width <width>    Output width in terminal columns.
      --height <height>  Output height in terminal rows.
      --max-width <max-width>  Maximum width to use when --width is excluded
      --max-height <max-height>  Maximum height to use when --height is excluded
      --true-colour             Use 24-bit RGB colour. Some terminals don't support this.
      --true-color             Use 24-bit RGB color but you don't spell so good.
      --filter <filter>
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_width: Option<u32>,
    flag_height: Option<u32>,
    flag_max_width: Option<u32>,
    flag_max_height: Option<u32>,
    flag_true_colour: bool,
    flag_true_color: bool,
    flag_filter: Option<String>,
    arg_file: String,
}

#[derive(Debug)]
enum LoadImageError {
    SvgError(String),
    ImageError(image::ImageError),
}

impl std::fmt::Display for LoadImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            LoadImageError::SvgError(msg) => write!(f, "{}", msg),
            LoadImageError::ImageError(err) => err.fmt(f),
        }
    }
}
impl std::error::Error for LoadImageError {}
impl From<image::ImageError> for LoadImageError {
    fn from(e: image::ImageError) -> Self {
        LoadImageError::ImageError(e)
    }
}

fn get_image(path: &String) -> std::result::Result<DynamicImage, LoadImageError> {
    if path.ends_with(".svg") {
        let svg_root = usvg::Tree::from_file(path, &usvg::Options::default());
        if let Err(_) = svg_root {
            return Err(LoadImageError::SvgError("Failed to load svg".to_string()));
        }
        let svg_root = svg_root.unwrap();
        let svg_image = resvg::render(&svg_root, usvg::FitTo::Width(1000), None);
        if let Some(svg_image) = svg_image {
            let mut dyn_img = DynamicImage::new_rgba8(svg_image.width(), svg_image.height());
            let data = svg_image.data();
            for x in 0..svg_image.width() {
                for y in 0..svg_image.height() {
                    let ind: usize = ((y * svg_image.width() + x) * 4) as usize;
                    let r = data[ind];
                    let g = data[ind + 1];
                    let b = data[ind + 2];
                    let a = data[ind + 3];

                    dyn_img.put_pixel(x, y, image::Rgba([r, g, b, a]))
                }
            }
            return Ok(dyn_img);
        }
    }
    Ok(image::open(path)?)
}

fn get_filter(str: String) -> Option<imageops::FilterType> {
    match str.as_str() {
        "nearest" => Some(imageops::Nearest),
        "triangle" => Some(imageops::Triangle),
        "catmullrom" => Some(imageops::CatmullRom),
        "gaussian" => Some(imageops::Gaussian),
        "lanczos3" => Some(imageops::Lanczos3),
        _ => None,
    }
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());


    let filter = (&args.flag_filter)
        .as_ref()
        .map_or(imageops::Gaussian, |f| {
        get_filter(f.clone()).unwrap_or_else(|| {
            eprintln!("Unknow filter: {}",f);
            std::process::exit(-1)
        })
    });

    let img = get_image(&args.arg_file).unwrap_or_else(|e| {
        eprint!("{}", e);
        std::process::exit(-1)
    });
    let (orig_width, orig_height) = img.dimensions();
    let true_colour = args.flag_true_colour || args.flag_true_color;
    let (width, height) = determine_size(args, orig_width, orig_height);

    termpix::print_image(img, true_colour, width, height, filter);
}

fn determine_size(args: Args, orig_width: u32, orig_height: u32) -> (u32, u32) {
    match (args.flag_width, args.flag_height) {
        (Some(w), Some(h)) => (w, h * 2),
        (Some(w), None) => (w, scale_dimension(w, orig_height, orig_width)),
        (None, Some(h)) => (scale_dimension(h * 2, orig_width, orig_height), h * 2),
        (None, None) => {
            let size = terminal_size();

            if let Some((Width(terminal_width), Height(terminal_height))) = size {
                fit_to_size(
                    orig_width,
                    orig_height,
                    terminal_width as u32,
                    (terminal_height - 1) as u32,
                    args.flag_max_width,
                    args.flag_max_height,
                )
            } else {
                writeln!(std::io::stderr(), "Neither --width or --height specified, and could not determine terminal size. Giving up.").unwrap();
                std::process::exit(1);
            }
        }
    }
}

fn scale_dimension(other: u32, orig_this: u32, orig_other: u32) -> u32 {
    (orig_this as f32 * other as f32 / orig_other as f32 + 0.5) as u32
}

pub fn fit_to_size(
    orig_width: u32,
    orig_height: u32,
    terminal_width: u32,
    terminal_height: u32,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> (u32, u32) {
    let target_width = match max_width {
        Some(max_width) => min(max_width, terminal_width),
        None => terminal_width,
    };

    //2 pixels per terminal row
    let target_height = 2 * match max_height {
        Some(max_height) => min(max_height, terminal_height),
        None => terminal_height,
    };

    let calculated_width = scale_dimension(target_height, orig_width, orig_height);
    if calculated_width <= target_width {
        (calculated_width, target_height)
    } else {
        (
            target_width,
            scale_dimension(target_width, orig_height, orig_width),
        )
    }
}
