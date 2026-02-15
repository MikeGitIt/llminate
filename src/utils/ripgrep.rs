use std::process::{Command, Stdio};

/// Run ripgrep with the given arguments
pub fn run(args: &[String]) -> i32 {
    // First try to use system ripgrep
    match Command::new("rg")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) => {
            status.code().unwrap_or(1)
        }
        Err(_) => {
            // If ripgrep is not installed, fall back to basic grep functionality
            eprintln!("ripgrep (rg) is not installed. Please install it for better search functionality.");
            eprintln!("Visit: https://github.com/BurntSushi/ripgrep#installation");
            
            // Try to use grep as fallback
            match Command::new("grep")
                .args(convert_rg_to_grep_args(args))
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
            {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => {
                    eprintln!("Neither ripgrep nor grep is available.");
                    1
                }
            }
        }
    }
}

/// Convert ripgrep arguments to grep arguments (basic conversion)
fn convert_rg_to_grep_args(rg_args: &[String]) -> Vec<String> {
    let mut grep_args = Vec::new();
    let mut skip_next = false;
    
    for (i, arg) in rg_args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        
        match arg.as_str() {
            "-i" | "--ignore-case" => grep_args.push("-i".to_string()),
            "-v" | "--invert-match" => grep_args.push("-v".to_string()),
            "-w" | "--word-regexp" => grep_args.push("-w".to_string()),
            "-n" | "--line-number" => grep_args.push("-n".to_string()),
            "-H" | "--with-filename" => grep_args.push("-H".to_string()),
            "-r" | "--recursive" => grep_args.push("-r".to_string()),
            "-F" | "--fixed-strings" => grep_args.push("-F".to_string()),
            "-E" | "--extended-regexp" => grep_args.push("-E".to_string()),
            arg if arg.starts_with("--max-count=") => {
                if let Some(count) = arg.strip_prefix("--max-count=") {
                    grep_args.push("-m".to_string());
                    grep_args.push(count.to_string());
                }
            }
            "-m" => {
                grep_args.push("-m".to_string());
                if i + 1 < rg_args.len() {
                    grep_args.push(rg_args[i + 1].clone());
                    skip_next = true;
                }
            }
            arg if arg.starts_with('-') => {
                // Unknown ripgrep flag, skip it
                eprintln!("Warning: Ignoring ripgrep-specific flag: {}", arg);
            }
            _ => {
                // Pattern or file/directory
                grep_args.push(arg.clone());
            }
        }
    }
    
    grep_args
}