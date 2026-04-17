use std::io::{self, BufRead, Write};

use cljrs::{builtins, env::Env, eval, reader};

fn main() {
    let env = Env::new();
    builtins::install(&env);

    let stdin = io::stdin();
    let stdout = io::stdout();

    print!("cljrs> ");
    stdout.lock().flush().ok();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        match reader::read_all(&line) {
            Err(e) => eprintln!("{e}"),
            Ok(forms) => {
                for form in forms {
                    match eval::eval(&form, &env) {
                        Ok(v) => println!("{v}"),
                        Err(e) => {
                            eprintln!("{e}");
                            break;
                        }
                    }
                }
            }
        }
        print!("cljrs> ");
        stdout.lock().flush().ok();
    }
}
