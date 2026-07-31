# Ghostscript reference renderer for the TPT Glyph visual-diff harness.
#
# Builds/runs upstream Ghostscript so CI can produce reference PNGs from the
# fixture corpus, which `tools/tpt-glyph-diff` then compares against TPT Glyph output.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ghostscript ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

# Render every input in /work/input to /work/output as a PNG at the given DPI.
# Usage (from repo root):
#   docker build -f docker/ghostscript/Dockerfile -t glyph-gs docker/ghostscript
#   docker run --rm -v "$PWD/fixtures:/work" glyph-gs render.ps 72
ENTRYPOINT ["gs"]
CMD ["-dNOPAUSE", "-dBATCH", "-sDEVICE=png16m", "-r72", \
     "-sOutputFile=/work/output/%08d.png", "/work/input/render.ps"]
