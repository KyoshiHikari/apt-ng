# Shell Completions for apt-ng

This directory contains installation scripts and instructions for shell completions.

## Supported Shells

- **zsh** - Full completion support
- **fish** - Full completion support  
- **bash** - Full completion support
- **PowerShell** - Full completion support

## Installation

### Automatic Installation

Run the installation script:

```bash
./completions/install.sh
```

This will:
1. Detect your current shell
2. Generate the appropriate completion file
3. Install it to the correct location
4. Provide instructions for enabling it

### Manual Installation

#### zsh

```bash
# Generate completion file
APT_NG_GENERATE_COMPLETIONS=zsh apt-ng > ~/.zsh/completions/_apt-ng

# Add to ~/.zshrc
fpath=($HOME/.zsh/completions $fpath)
autoload -U compinit && compinit
```

#### fish

```bash
# Generate completion file
APT_NG_GENERATE_COMPLETIONS=fish apt-ng > ~/.config/fish/completions/apt-ng.fish
```

Fish will automatically load completions from `~/.config/fish/completions/`.

#### bash

```bash
# Generate completion file
APT_NG_GENERATE_COMPLETIONS=bash apt-ng > ~/.bash_completion.d/apt-ng

# Add to ~/.bashrc
source ~/.bash_completion.d/apt-ng
```

#### PowerShell

```powershell
# Generate completion file
$env:APT_NG_GENERATE_COMPLETIONS="powershell"
apt-ng | Out-File -FilePath "$PROFILE\apt-ng.ps1"
```

## Usage

After installation, completions will be available automatically:

```bash
apt-ng <TAB>          # Shows all available commands
apt-ng install <TAB>   # Shows available packages
apt-ng repo <TAB>      # Shows repo subcommands
```

## Troubleshooting

If completions don't work:

1. Make sure the completion file was generated correctly
2. Check that your shell configuration file (`.zshrc`, `.bashrc`, etc.) includes the necessary setup
3. Restart your shell or source the configuration file
4. Verify the completion file exists and is readable

For zsh, you may need to rebuild the completion cache:

```bash
rm ~/.zcompdump*
compinit
```

