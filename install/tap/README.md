# DonaldMurillo/homebrew-statico

Homebrew tap for [statico](https://github.com/DonaldMurillo/statico) — a static code analyzer for TypeScript and Rust projects.

## Install

```bash
brew tap DonaldMurillo/statico
brew install statico
```

## Upgrade

```bash
brew upgrade statico
```

## Uninstall

```bash
brew uninstall statico
brew untap DonaldMurillo/statico
```

## Shell completions

Completions for **bash**, **zsh**, and **fish** are installed automatically by the formula.

### Bash

If you use Homebrew's bash-completion:

```bash
brew install bash-completion@2
```

Completions are loaded from `$(brew --prefix)/etc/bash_completion.d/`.

### Zsh

Make sure Homebrew's completions directory is in your `fpath`. Add this to `~/.zshrc` **before** `compinit`:

```zsh
if type brew &>/dev/null; then
  fpath+=($(brew --prefix)/share/zsh/site-functions)
fi
```

Then restart your shell or run:

```bash
exec zsh
```

### Fish

Fish completions are loaded automatically from `$(brew --prefix)/share/fish/vendor_completions.d/`.

## Alternative: curl | sh installer

If you prefer not to use Homebrew:

```bash
curl -fsSL https://github.com/DonaldMurillo/statico/raw/main/install/install.sh | sh
```

To uninstall:

```bash
curl -fsSL https://github.com/DonaldMurillo/statico/raw/main/install/install.sh | sh -s -- --uninstall
```
