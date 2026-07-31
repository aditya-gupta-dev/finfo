fn get_process_info(system: &sysinfolinux::System, pid: sysinfolinux::Pid) {
    match system.process(pid) {
        Some(proc) => {
            print_process_info(&proc);
        }
        None => {
            println!("failed to get process info: {} maybe not running...", pid);
        }
    }
}

pub fn run(pids: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let system = sysinfolinux::System::new_all()?;

    if pids.is_empty() {
        let processes = system.processes();
        for (_, process) in processes {
            print_process_info(process);
            println!("total: {}", processes.len());
        }
    }

    for pid in pids {
        let pid = pid.parse::<usize>()?;
        get_process_info(&system, sysinfolinux::Pid::from(pid));
    }
    Ok(())
}

fn print_process_info(proc: &sysinfolinux::Process) {
    println!("pid: {}", proc.pid());
    println!("\tname: {:?}", proc.name());
    println!("\tcmd: {:?}:", proc.cmd());
    println!("\texe: {:?}", proc.exe());
    println!("\tstatus: {:?}", proc.status());
    println!("\truntime: {} secs", proc.run_time());
    println!("\tmem_usage: {}", format_size(proc.memory()));
    println!("\tvirt_mem_usage: {}", format_size(proc.virtual_memory()));
    println!("\tcpu_usage: {}", proc.cpu_usage());
    println!("\tcpu_time: {} secs", proc.accumulated_cpu_time());
    println!("\tstarted_at: {}", proc.start_time());
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}
