fn main() {
    // Embed Windows manifest for visual styles
    if cfg!(target_os = "windows") {
        // Compile the .rc resource script (which references the manifest)
        embed_resource::compile("awclippy.rc", Vec::<String>::new());
    }
}
