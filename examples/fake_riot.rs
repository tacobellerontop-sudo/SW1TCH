//! Stand-in for RiotClientServices.exe used by the Windows smoke test:
//! records its arguments next to itself, then idles until it is killed.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let log = std::env::current_exe().unwrap().with_file_name("launched.txt");
    std::fs::write(log, args.join(" ")).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(300));
}
