class Statico < Formula
  desc "Static code analyzer for TypeScript and Rust projects"
  homepage "https://github.com/DonaldMurillo/statico"
  url "https://github.com/DonaldMurillo/statico/releases/download/v0.1.0/statico-macos-#{Hardware::CPU.arch}.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  version "0.1.0"

  def install
    bin.install "statico"

    # Bash completions
    bash_completion.mkpath
    output = Utils.safe_popen_read("#{bin}/statico", "completions", "bash")
    (bash_completion/"statico").write output

    # Zsh completions
    zsh_completion.mkpath
    output = Utils.safe_popen_read("#{bin}/statico", "completions", "zsh")
    (zsh_completion/"_statico").write output

    # Fish completions
    fish_completion.mkpath
    output = Utils.safe_popen_read("#{bin}/statico", "completions", "fish")
    (fish_completion/"statico.fish").write output
  end

  test do
    assert_match "statico", shell_output("#{bin}/statico --version", 0)
  end
end