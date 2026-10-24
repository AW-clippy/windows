fn main() {
    // Embed Windows manifest for visual styles
    if cfg!(target_os = "windows") {
        // Compile the .rc resource script (which references the manifest)
        embed_resource::compile("awclippy.rc", Vec::<String>::new());
    }

    // Read .env file at compile-time and bake configuration directly into binary
    if let Ok(env_content) = std::fs::read_to_string(".env") {
        for line in env_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = trimmed.split_once('=') {
                let key = key.trim();
                let mut val = val.trim();
                if (val.starts_with('\'') && val.ends_with('\'')) || (val.starts_with('"') && val.ends_with('"')) {
                    if val.len() >= 2 {
                        val = &val[1..val.len() - 1];
                    }
                }
                match key {
                    "AWCLIPPY_SHARED_SECRET" => println!("cargo:rustc-env=COMPILED_AWCLIPPY_SHARED_SECRET={}", val),
                    "AWCLIPPY_DISCOVERY_PORT" => println!("cargo:rustc-env=COMPILED_AWCLIPPY_DISCOVERY_PORT={}", val),
                    "AWCLIPPY_SYNC_PORT" => println!("cargo:rustc-env=COMPILED_AWCLIPPY_SYNC_PORT={}", val),
                    "AWCLIPPY_VERSION" => println!("cargo:rustc-env=COMPILED_AWCLIPPY_VERSION={}", val),
                    _ => {}
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=.env");
}
