#![allow(clippy::print_stdout)]
//! Reference comparison between kynex-icons and GTK icon lookup.
use kynex_icons::lookup::IconResolver;
use kynex_icons::roots::IconRoots;
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let theme_name = args
        .first()
        .cloned()
        .unwrap_or_else(|| "Adwaita".to_string());
    let icons: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        vec![]
    };

    let roots = IconRoots::from_env();
    let resolver = IconResolver::new(roots);

    for icon in &icons {
        let res = resolver.find_icon(icon, 48, 1, &theme_name);
        match res {
            Ok(Some(path)) => println!("KYNEX\t{}\t{}", icon, path.display()),
            Ok(None) => println!("KYNEX\t{icon}\tNONE"),
            Err(e) => println!("KYNEX\t{icon}\tERR: {e:?}"),
        }
    }
}
