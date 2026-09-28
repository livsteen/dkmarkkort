fn main() {
    topcoat::tailwind::BuildConfig::new()
        .input("stil.css")
        .render()
        .unwrap();
}
