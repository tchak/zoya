use clap::Parser;

#[derive(Parser)]
#[command(name = "zoya")]
#[command(version, about = "The Zoya programming language")]
struct Cli {
    // Future arguments/subcommands will go here
}

fn main() {
    let _cli = Cli::parse();

    // For now, just print a welcome message if no args
    println!("Zoya language - use --help for usage");
}
