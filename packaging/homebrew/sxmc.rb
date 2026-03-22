class Sxmc < Formula
  desc "AI-agnostic Skills x MCP x CLI pipeline"
  homepage "https://github.com/aihxp/sxmc"
  url "https://github.com/aihxp/sxmc/archive/refs/tags/v0.2.13.tar.gz"
  sha256 "13df5ce06ad8987bb9e64c181bbe67d6f14c007611c96ca8d8460eebff79b23b"
  license "MIT"
  head "https://github.com/aihxp/sxmc.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match "sxmc", shell_output("#{bin}/sxmc --version")
  end
end
