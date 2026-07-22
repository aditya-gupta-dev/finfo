fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect(); 
    if let Err(err) = finfo::run(&files) { 
        println!("{}", err);
    }
}
