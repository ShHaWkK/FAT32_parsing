use fat32_parser::Fat32;
use std::env;
use std::fs;
use std::io::{self, Write};

/// Affiche un message d'aide pour la ligne de commande.
fn print_usage() {
    eprintln!(
        "Usage:
  fat32_cli --file <dump_fat32> [--ls <chemin>] [--cat <chemin_fichier>]

Exemples :
  fat32_cli --file disk.img
  fat32_cli --file disk.img --ls /
  fat32_cli --file disk.img --ls /DIR
  fat32_cli --file disk.img --cat /HELLO.TXT

Sans --ls / --cat, un petit shell interactif est lancé :
  ls [chemin]
  cat <chemin_fichier>
  help
  exit"
    );
}

/// Affiche l'aide du mini-shell interactif.
fn print_shell_help() {
    println!(
        "Commandes disponibles :
  ls [chemin]       - Liste le contenu d'un répertoire (par défaut : /)
  cat <chemin>      - Affiche le contenu d'un fichier
  help              - Affiche ce message
  exit              - Quitte le shell"
    );
}

fn main() {
    let mut args = env::args().skip(1);

    let mut dump_path: Option<String> = None;
    let mut command: Option<String> = None;
    let mut target_path: Option<String> = None;

    // Parsing très simple des arguments.
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
            let path = target_path.as_deref().unwrap_or("/");
            run_ls(&fs, path);
        }
        Some("cat") => {
            let path = match target_path {
                Some(p) => p,
                None => {
                    eprintln!("--cat nécessite un chemin de fichier");
                    print_usage();
                    return;
                }
            };
            run_cat(&fs, &path);
        }
        Some(other) => {
            eprintln!("Commande inconnue : {other}");
            print_usage();
            return;
        }
        // … sinon on lance un mini-shell interactif.
        None => {
            run_shell(&fs);
        }
    }
}

/// Exécute une commande `ls` sur un chemin donné (mode non interactif).
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

/// Exécute une commande `cat` sur un fichier donné (mode non interactif).
fn run_cat(fs: &Fat32, path: &str) {
    match fs.read_file_by_path(path) {
        Ok(Some(bytes)) => {
            // Affichage brut (souvent du texte).
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

/// Lance un mini-shell interactif sur le volume FAT32.
///
/// Exemple de session :
///
/// ```text
/// fat32> ls
/// fat32> ls /DIR
/// fat32> cat /HELLO.TXT
/// fat32> exit
/// ```
fn run_shell(fs: &Fat32) {
    println!("FAT32 shell interactif. Tapez 'help' pour l'aide, 'exit' pour quitter.");

    let stdin = io::stdin();
    loop {
        print!("fat32> ");
        // Important : flush pour afficher le prompt tout de suite.
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

        // Parsing très simple : commande + argument optionnel.
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap();

        match cmd {
            "exit" | "quit" => {
                break;
            }
            "help" => {
                print_shell_help();
            }
            "ls" => {
                let path = parts.next().unwrap_or("/");
                run_ls(fs, path);
            }
            "cat" => {
                if let Some(path) = parts.next() {
                    run_cat(fs, path);
                } else {
                    println!("Usage: cat <chemin_fichier>");
                }
            }
            _ => {
                println!("Commande inconnue : {cmd}. Tapez 'help' pour la liste des commandes.");
            }
        }
    }
}
