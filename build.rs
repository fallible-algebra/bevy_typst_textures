fn main() {
    println!("cargo::rerun-if-changed=assets/");
    if cfg!(target_os = "macos") {
        // ditto -c -k --sequesterRsrc --keepParent src_directory archive.zip
        // run this only when doing development on the repo.
        // use std::process::Command;
        // let _ = Command::new("ditto")
        //     .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        //     .args(["assets/example", "assets/example.zip"])
        //     .output();
    } else {
        // TODO: other compression tools
    };
}
