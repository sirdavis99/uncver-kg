class KgCore < Formula
  desc "Knowledge Graph Memory Engine - Persistent LLM memory with 3-agent architecture"
  homepage "https://github.com/yourusername/kg-core"
  url "https://github.com/yourusername/kg-core/archive/refs/tags/v#{version}.tar.gz"
  sha256 "REPLACEME"
  license "MIT"

  head "https://github.com/yourusername/kg-core.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix
  end

  test do
    system "#{bin}/kg-core", "--version"
  end
end
