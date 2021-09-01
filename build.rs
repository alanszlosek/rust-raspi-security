extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to tell rustc to link the mmal shared libraries
    println!("cargo:rustc-link-lib=mmal_core");
    println!("cargo:rustc-link-lib=mmal_util");
    println!("cargo:rustc-link-lib=mmal_vc_client");
    println!("cargo:rustc-link-lib=vcos");
    println!("cargo:rustc-link-lib=bcm_host");
    //println!("cargo:rustc-link-lib=mmal");


    //// Tell cargo where to find the libraries
    println!("cargo:rustc-link-search=native={}", "/opt/vc/lib");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))

        // Only include mmal stuff
        .allowlist_type(r"(MMAL|VCOS)_.*")
        .allowlist_function(r"(mmal_|vcos_|bcm_).*")
        .allowlist_var(r"(MMAL|VCOS)_.*")

        // Tell it where to find the MMAL includes
        // add mmal too, since some files are relative, oddly

        // this needs to go early since other headers include "mmal_events.h" from subdirectories, which doesn't work
        .clang_arg("-I/opt/vc/include/interface/mmal")
        .clang_arg("-I/opt/vc/include")

        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
