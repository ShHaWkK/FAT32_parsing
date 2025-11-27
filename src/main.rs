use fat32_parser::Fat32;
use std::env;
use std::fs;
use std::io::{self, Write};

/// Affiche un message d'aide pour la ligne de commande.
fn print_usage() {
    eprintln!(
        "Exemples :
  fat32_cli --file disk.img
  fat32_cli --file disk.img --ls /
  fat32_cli --file disk.img --ls DIR
  fat32_cli --file disk.img --cat HELLO.TXT"
    );
}

/// Affiche l'aide du mini-shell
fn print_shell_help() {
    println!(
        "Commandes disponibles :
  ls [chemin]       - Liste le contenu d'un répertoire (absolu ou relatif)
  cat <chemin>      - Affiche le contenu d'un fichier
  cd [chemin]       - Change de répertoire courant (par défaut : /)
  pwd               - Affiche le répertoire courant
  help              - Affiche ce message
  exit              - Quitte le shell"
    );
}

fn main() {
    let mut args = env::args().skip(1);

    let mut dump_path: Option<String> = None;
    let mut command: Option<String> = None;
    let mut target_path: Option<String> = None;

    // Parsing des arguments.
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" | "-f" => {
                dump_path = args.next();
            }
            "--ls" => {
                command = Some("ls".to_string());
                target_path = args.next();
            }
            "--cat" => {
                command = Some("cat".to_string());
                target_path = args.next();
            }
            _ => {
                eprintln!("Argument inconnu : {arg}");
                print_usage();
                return;
            }
        }
    }

    let dump_path = match dump_path {
        Some(p) => p,
        None => {
            print_usage();
            return;
        }
    };

    // Lecture du dump FAT32 en mémoire.
    let data =
        fs::read(&dump_path).expect("Impossible de lire le fichier dump");

    let fs = match Fat32::new(&data) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!(
                "Erreur lors de l'analyse du dump FAT32: {:?}.
Le fichier est-il bien formaté en FAT32 ?",
                e
            );
            return;
        }
    };

    // Si une commande est passée en arguments, on l'exécute une fois…
    match command.as_deref() {
        Some("ls") => {
            // ici on autorise directement un chemin absolu ou relatif depuis /
            let cwd = "/";
            let path = target_path
                .as_deref()
                .map(|p| resolve_path(cwd, p))
                .unwrap_or_else(|| cwd.to_string());
            run_ls(&fs, &path);
        }
        Some("cat") => {
            let cwd = "/";
            let rel = match target_path {
                Some(p) => p,
                None => {
                    eprintln!("--cat nécessite un chemin de fichier");
                    print_usage();
                    return;
                }
            };
            let path = resolve_path(cwd, &rel);
            run_cat(&fs, &path);
        }
        Some(other) => {
            eprintln!("Commande inconnue : {other}");
            print_usage();
        }
        // sinon on lance le mini-shell
        None => {
            run_shell(&fs);
        }
    }
}

/// Résout un chemin à partir d'un répertoire courant.
///
/// Si path commence par "/", il est traité comme absolu
/// Sinon, il est interprété relativement à current
/// On gère aussi "." et ".." de façon classique.
fn resolve_path(current: &str, path: &str) -> String {
    let mut components = Vec::new();

    // Si le chemin est absolu, on ignore le current.
    if path.starts_with('/') {
        for part in path.split('/') {
            push_component(&mut components, part);
        }
    } else {
        // On part du current (qui est supposé absolu).
        for part in current.split('/') {
            push_component(&mut components, part);
        }
        for part in path.split('/') {
            push_component(&mut components, part);
        }
    }

    if components.is_empty() {
        "/".to_string()
    } else {
        let mut result = String::from("/");
        result.push_str(&components.join("/"));
        result
    }
}

/// Gère les composants de chemin ("" / "." / ".." / nom normal).
fn push_component(components: &mut Vec<&str>, part: &str) {
    match part {
        "" | "." => {}
        ".." => {
            components.pop();
        }
        _ => components.push(part),
    }
}

/// Exécute une commande "ls" sur un chemin **absolu** donné.
fn run_ls(fs: &Fat32, path: &str) {
    match fs.list_dir_path(path) {
        Ok(entries) => {
            println!("Listing de {path}:");
            for e in entries {
                let kind = if e.is_dir() { "DIR " } else { "FILE" };
                println!("{kind} {:<24} {:>8} bytes", e.name, e.size);
            }
        }
        Err(e) => {
            eprintln!("Erreur list_dir_path({path:?}): {:?}", e);
        }
    }
}

/// Exécute une commande "cat" sur un chemin **absolu** donné.
fn run_cat(fs: &Fat32, path: &str) {
    match fs.read_file_by_path(path) {
        Ok(Some(bytes)) => {
            // Affichage brut
            print!("{}", String::from_utf8_lossy(&bytes));
        }
        Ok(None) => {
            eprintln!("Fichier introuvable : {path}");
        }
        Err(e) => {
            eprintln!("Erreur read_file_by_path({path:?}): {:?}", e);
        }
    }
}

/// Lance un mini-shell sur le volume FAT32.
///
/// On gère :"ls", "cat", "cd", "pwd", "help", "exit"
fn run_shell(fs: &Fat32) {
    println!("FAT32 shell interactif. Tapez 'help' pour l'aide, 'exit' pour quitter.");

    let stdin = io::stdin();
    let mut current_dir = String::from("/");

    loop {
        print!("fat32:{current_dir}> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        let n = match stdin.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => break,
        };

        if n == 0 {
            // EOF
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap();

        match cmd {
            "exit" | "quit" => {
                break;
            }
            "help" => {
                print_shell_help();
            }
            "pwd" => {
                println!("{current_dir}");
            }
            "ls" => {
                let path = if let Some(p) = parts.next() {
                    resolve_path(&current_dir, p)
                } else {
                    current_dir.clone()
                };
                run_ls(fs, &path);
            }
            "cat" => {
                if let Some(p) = parts.next() {
                    let path = resolve_path(&current_dir, p);
                    run_cat(fs, &path);
                } else {
                    println!("Usage: cat <chemin_fichier>");
                }
            }
            "cd" => {
                let target = if let Some(p) = parts.next() {
                    resolve_path(&current_dir, p)
                } else {
                    "/".to_string()
                };

                match fs.open_path(&target) {
                    Ok(Some(entry)) if entry.is_dir() => {
                        current_dir = target;
                    }
                    Ok(Some(_)) => {
                        println!("{target} n'est pas un répertoire");
                    }
                    Ok(None) => {
                        println!("Répertoire introuvable : {target}");
                    }
                    Err(e) => {
                        println!("Erreur cd vers {target:?}: {:?}", e);
                    }
                }
            }
            _ => {
                println!("Commande inconnue : {cmd}. Tapez 'help' pour la liste des commandes");
            }
        }
    }
}
