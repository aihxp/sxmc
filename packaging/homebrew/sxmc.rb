class Sxmc < Formula
  desc "Sumac: bring out what your tools can do (Skills x MCP x CLI)"
  homepage "https://github.com/aihxp/sumac"
  url "https://github.com/aihxp/sumac/archive/refs/tags/v0.2.38.tar.gz"
  sha256 "873f1569f13ab4a8ae2804938c257dd8ff732d66a1aa05d155bd1518734a1bbe"
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
