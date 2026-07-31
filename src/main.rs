fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 {
        return;
    }

    let search_type = &args[1];

    if search_type.eq("--pids") {
        let pids: Vec<String> = std::env::args().skip(2).collect(); 
        if let Err(err) = finfo::pids::run(&pids) { 
            eprintln!("{}", err); 
        }
    } else if search_type.eq("--files") {
        let files: Vec<String> = std::env::args().skip(2).collect();
        if let Err(err) = finfo::files::run(&files) {
            eprintln!("{}", err);
        }
    } else {
        println!("unknown option provided.");
    }

    // if let Err(err) = finfo::run(&files) {
    //    println!("{}", err);
    // }
}
