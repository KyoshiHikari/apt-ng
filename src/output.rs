use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use atty::Stream;
use comfy_table::{Table, Cell, presets::UTF8_FULL, ContentArrangement};

/// Formatted output for apt-ng
pub struct Output;

impl Output {
    /// Check if colors should be enabled
    pub(crate) fn colors_enabled() -> bool {
        atty::is(Stream::Stdout) && std::env::var("NO_COLOR").is_err()
    }
    
    /// Disable colors if terminal doesn't support them
    fn disable_colors_if_needed() {
        if !Self::colors_enabled() {
            colored::control::set_override(false);
        }
    }
    
    /// Initialize output system (call at startup)
    pub fn init() {
        Self::disable_colors_if_needed();
    }
    
    /// Show a success message
    pub fn success(msg: &str) {
        if Self::colors_enabled() {
            println!("{} {}", "✓".green().bold(), msg.green());
        } else {
            println!("✓ {}", msg);
        }
    }
    
    /// Show an error message
    pub fn error(msg: &str) {
        if Self::colors_enabled() {
            eprintln!("{} {}", "✗".red().bold(), msg.red());
        } else {
            eprintln!("✗ {}", msg);
        }
    }
    
    /// Show a warning message
    pub fn warning(msg: &str) {
        if Self::colors_enabled() {
            println!("{} {}", "⚠".yellow().bold(), msg.yellow());
        } else {
            println!("⚠ {}", msg);
        }
    }
    
    /// Show an info message
    pub fn info(msg: &str) {
        if Self::colors_enabled() {
            println!("{} {}", "ℹ".cyan().bold(), msg.cyan());
        } else {
            println!("ℹ {}", msg);
        }
    }
    
    /// Show a heading
    pub fn heading(msg: &str) {
        if Self::colors_enabled() {
            println!("\n{}", msg.bold().bright_blue());
        } else {
            println!("\n{}", msg);
        }
    }
    
    /// Show a section header
    pub fn section(msg: &str) {
        if Self::colors_enabled() {
            println!("{}", msg.bold());
        } else {
            println!("{}", msg);
        }
    }
    
    /// Show a list item
    pub fn list_item(item: &str) {
        if Self::colors_enabled() {
            println!("  {} {}", "•".cyan(), item);
        } else {
            println!("  • {}", item);
        }
    }
    
    /// Create a table for displaying data
    pub fn table() -> Table {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        // Make table responsive to terminal width
        if let Some((_width, _)) = term_size::dimensions() {
            table.set_content_arrangement(ContentArrangement::Dynamic);
        }
        table
    }
    
    /// Show package search results in a table
    pub fn package_table(packages: &[(&str, &str, &str)]) {
        let mut table = Self::table();
        table.set_header(vec!["Package", "Version", "Architecture"]);
        
        for (name, version, arch) in packages {
            let name_cell = if Self::colors_enabled() {
                Cell::new(name).fg(comfy_table::Color::Cyan)
            } else {
                Cell::new(name)
            };
            table.add_row(vec![name_cell, Cell::new(version), Cell::new(arch)]);
        }
        
        println!("{}", table);
    }
    
    /// Create a progress bar
    pub fn progress_bar(len: u64) -> ProgressBar {
        let pb = ProgressBar::new(len);
        
        // Use different styles based on terminal capabilities
        let template = if Self::colors_enabled() {
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({percent}%) {msg}"
        } else {
            "[{elapsed_precise}] [{bar:40}] {pos}/{len} ({percent}%) {msg}"
        };
        
        pb.set_style(
            ProgressStyle::default_bar()
                .template(template)
                .unwrap()
                .progress_chars(if Self::colors_enabled() { "#>-" } else { "=>-" })
        );
        pb
    }
    
    /// Create a simple progress bar without template
    #[allow(dead_code)]
    pub fn simple_progress_bar(len: u64) -> ProgressBar {
        ProgressBar::new(len)
    }
    
    /// Show formatted package information
    pub fn package_info(name: &str, version: &str, arch: &str) {
        if Self::colors_enabled() {
            println!("  {} {} {} ({})", 
                "📦".cyan(), 
                name.bold(), 
                version.dimmed(), 
                arch.dimmed()
            );
        } else {
            println!("  📦 {} {} ({})", name, version, arch);
        }
    }
    
    /// Show download information
    pub fn download_info(name: &str, size: &str) {
        if Self::colors_enabled() {
            println!("  {} {} {}", 
                "⬇".green(), 
                format!("Downloading {}", name).bold(),
                format!("({})", size).dimmed()
            );
        } else {
            println!("  ⬇ Downloading {} ({})", name, size);
        }
    }
    
    /// Show installation information
    pub fn install_info(name: &str, version: &str) {
        if Self::colors_enabled() {
            println!("  {} {} {}", 
                "🔧".magenta(), 
                format!("Installing {}", name).bold(),
                version.dimmed()
            );
        } else {
            println!("  🔧 Installing {} {}", name, version);
        }
    }
    
    /// Show repository information
    pub fn repo_info(url: &str) {
        if Self::colors_enabled() {
            println!("  {} {}", "📥".blue(), url.bold());
        } else {
            println!("  📥 {}", url);
        }
    }
    
    /// Show a summary
    pub fn summary(title: &str, count: usize) {
        if Self::colors_enabled() {
            println!("\n{} {} {}", 
                "✓".green().bold(),
                title.bold(),
                format!("{} package(s)", count).green()
            );
        } else {
            println!("\n✓ {} {} package(s)", title, count);
        }
    }
    
    /// Show a progress message (for verbose output)
    pub fn progress_message(msg: &str) {
        if Self::colors_enabled() {
            print!("\r{} {}", "⏳".yellow(), msg.dimmed());
            use std::io::Write;
            let _ = std::io::stdout().flush();
        } else {
            print!("\r⏳ {}", msg);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }
    
    /// Show a URL
    #[allow(dead_code)]
    pub fn url(url: &str) {
        if Self::colors_enabled() {
            println!("  {} {}", "🔗".blue(), url.dimmed());
        } else {
            println!("  🔗 {}", url);
        }
    }
}

