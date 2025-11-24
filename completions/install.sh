#!/bin/bash
# Installation script for apt-ng shell completions

# Detect shell
if [ -n "$ZSH_VERSION" ]; then
    SHELL_NAME="zsh"
    COMPLETION_DIR="${HOME}/.zsh/completions"
elif [ -n "$FISH_VERSION" ]; then
    SHELL_NAME="fish"
    COMPLETION_DIR="${HOME}/.config/fish/completions"
elif [ -n "$BASH_VERSION" ]; then
    SHELL_NAME="bash"
    COMPLETION_DIR="${HOME}/.bash_completion.d"
else
    echo "Unsupported shell. Please use zsh, fish, or bash."
    exit 1
fi

# Create completion directory if it doesn't exist
mkdir -p "$COMPLETION_DIR"

# Generate completion file
APT_NG_GENERATE_COMPLETIONS="$SHELL_NAME" apt-ng > "$COMPLETION_DIR/_apt-ng" 2>/dev/null

if [ $? -eq 0 ]; then
    echo "✓ Completion file installed to $COMPLETION_DIR/_apt-ng"
    
    if [ "$SHELL_NAME" = "zsh" ]; then
        echo "Add this to your ~/.zshrc:"
        echo "  fpath=(\$HOME/.zsh/completions \$fpath)"
        echo "  autoload -U compinit && compinit"
    elif [ "$SHELL_NAME" = "fish" ]; then
        echo "Fish will automatically load completions from ~/.config/fish/completions/"
    elif [ "$SHELL_NAME" = "bash" ]; then
        echo "Add this to your ~/.bashrc:"
        echo "  source ~/.bash_completion.d/_apt-ng"
    fi
else
    echo "✗ Failed to generate completion file"
    exit 1
fi

