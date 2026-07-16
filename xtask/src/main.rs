use std::env;
use std::fs;
use std::io;
use std::path::Path;

use clap::Command;
use clap_complete::{Shell, generate_to};

fn main() -> io::Result<()> {
    match env::args().nth(1).as_deref() {
        Some("release-assets") => generate_release_assets(),
        Some(command) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown xtask `{command}`; expected `release-assets`"),
        )),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing xtask; expected `release-assets`",
        )),
    }
}

fn generate_release_assets() -> io::Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a direct child of the workspace");
    let output = workspace.join("assets");
    let completions = output.join("completions");
    let man = output.join("man");
    fs::create_dir_all(&completions)?;
    fs::create_dir_all(&man)?;

    for shell in [Shell::Bash, Shell::Elvish, Shell::Fish, Shell::PowerShell, Shell::Zsh] {
        generate_to(shell, &mut codeplat::command(), "codeplat", &completions)?;
    }
    render_man_pages(codeplat::command(), Vec::new(), &man)
}

fn render_man_pages(command: Command, parents: Vec<String>, output: &Path) -> io::Result<()> {
    let subcommands = command.get_subcommands().cloned().collect::<Vec<_>>();
    let mut path = parents;
    path.push(command.get_name().to_owned());
    let page = path.join("-");
    let display = path.join(" ");
    let command = command.name(leak(page.clone())).bin_name(leak(display));
    let mut rendered = Vec::new();
    clap_mangen::Man::new(command).render(&mut rendered)?;
    fs::write(output.join(format!("{page}.1")), rendered)?;

    for subcommand in subcommands {
        render_man_pages(subcommand, path.clone(), output)?;
    }
    Ok(())
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
