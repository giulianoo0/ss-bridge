fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/app.ico");
        res.compile().expect("windows resources");
    }
}
