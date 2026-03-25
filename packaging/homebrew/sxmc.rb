class Sxmc < Formula
  desc "Sumac: bring out what your tools can do (Skills x MCP x CLI)"
  homepage "https://github.com/aihxp/sumac"
  url "https://github.com/aihxp/sumac/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "88ead89b164898cef4005d91fe6209039b755abf65b31def264d9dd5bbf40d22"
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
