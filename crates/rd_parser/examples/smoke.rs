use std::env;
use std::fs;
use ubel_stratum::ast::arena::AstArena;

fn main() {
    let dir = env::args().nth(1).unwrap_or_else(|| "../../tests/fixtures".to_string());
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", dir, e))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "ubl").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());

    let mut ok = 0;
    let mut fail = 0;

    for entry in entries {
        let path = entry.path();
        let source = fs::read_to_string(&path).unwrap();
        let tokens = match ubel_stratum::lexer::tokenize(&source) {
            Ok(t) => t,
            Err(e) => {
                println!("LEX-FAIL  {:?}: {:?}", path.file_name().unwrap(), e);
                fail += 1;
                continue;
            }
        };
        let arena = AstArena::new();
        match ubel_stratum_rd::parse(&arena, &tokens, source.clone()) {
            Ok(program) => {
                println!("OK        {:?}  ({} items)", path.file_name().unwrap(), program.items.len());
                ok += 1;
            }
            Err(mut errs) => {
                println!("PARSE-ERR {:?}", path.file_name().unwrap());
                for e in errs.take_parse_errors() {
                    println!("            {:?}", e);
                }
                fail += 1;
            }
        }
    }

    println!("\n{} ok, {} fail", ok, fail);
      }
