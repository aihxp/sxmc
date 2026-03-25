class Sxmc < Formula
  desc "Sumac: bring out what your tools can do (Skills x MCP x CLI)"
  homepage "https://github.com/aihxp/sumac"
  url "https://github.com/aihxp/sumac/archive/refs/tags/v0.2.42.tar.gz"
  sha256 "43c1b7430d105d7255843bf4c7f88869fe3c61883c9642891aaa8e0fff06349d"
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
