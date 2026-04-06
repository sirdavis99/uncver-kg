class Uncverkg < Formula
  desc "Knowledge Graph Memory Engine - Persistent LLM memory with 3-agent architecture"
  homepage "https://github.com/yourusername/uncverkg"
  url "https://github.com/yourusername/uncverkg/archive/refs/tags/v#{version}.tar.gz"
  sha256 "REPLACEME"
  license "MIT"

  head "https://github.com/yourusername/uncverkg.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix", "bin", "uncverkg"
  end

  test do
    system "#{bin}/uncverkg", "--version"
  end
end