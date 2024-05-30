
use std::{env};

fn env_var(var: &str) -> String {
    env::var(var).unwrap_or_else(|_| panic!("`{}` is not set", var))
}


fn main() {
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=VideoToolbox");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=OpenCL");
        // println!(r"cargo:rustc-link-search=/opt/homebrew/Cellar/sdl2/2.30.1/lib/");
    }

    // println!("cargo:rustc-link-lib=static=SDL2");
    // println!("cargo:rustc-link-lib=static=SDL2main");
    // println!("cargo:rustc-link-lib=static=SDL2_mixer");
    println!("cargo:rustc-link-lib=static=x264");
    println!("cargo:rustc-link-lib=static=fdk-aac");

    // let lib_path = PathBuf::from(env_var("CARGO_MANIFEST_DIR"));
    // println!(
    //     "cargo:rustc-link-search=native={}",
    //     lib_path.to_strResources/dll().unwrap()
    // );
    // let is_release = match &*env_var("PROFILE") {
    //     "debug" => false,
    //     "release" => true,
    //     _ => panic!("unexpected value set for PROFILE env"),
    // };

    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/dll");
}