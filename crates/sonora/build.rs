// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    #[cfg(windows)]
    {
        let icon = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/windows/sonora.ico"
        );
        println!("cargo:rerun-if-changed={icon}");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(icon);
        if let Err(error) = resource.compile() {
            println!("cargo:warning=cannot embed the windows icon: {error}");
        }
    }
}
