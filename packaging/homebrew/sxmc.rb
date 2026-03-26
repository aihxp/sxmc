class Sxmc < Formula
  desc "Sumac: bring out what your tools can do (Skills x MCP x CLI)"
  homepage "https://github.com/aihxp/sumac"
  url "https://github.com/aihxp/sumac/archive/refs/tags/v1.0.9.tar.gz"
  sha256 "710a81c098f005c64d987da5c625ba26ab16be727bc92e4bff8faadd56d098b1"
  license "MIT"
  head "https://github.com/aihxp/sumac.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match "sxmc", shell_output("#{bin}/sxmc --version")
  end
end
