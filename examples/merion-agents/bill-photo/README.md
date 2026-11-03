# bill-photo

A photo or screenshot of a utility, phone or card bill in, the money and the terms out. The cell reads the picture with OCR (`tesseract`, English) and the
same extractive rules as its PDF sibling run on the text. No model reads the image or the text.

```bash
./scripts/merionctl deploy examples/merion-agents/bill-photo
./scripts/merionctl run bill-photo photo.jpg
```

- Takes `.png`, `.jpg` / `.jpeg` and `.tif` / `.tiff`. **iPhone photos are often HEIC**: share them as JPEG (Photos, Share, Options, Most
  Compatible) or take a screenshot. Merion does not read HEIC.
- OCR is only as good as the picture: a straight-on, sharp, well-lit photo works; a curved, blurry or tiny one may give nothing or wrong
  characters. **Check the amounts against the original** before you rely on them. A run that reads no text is refused, not guessed.
- English text only for now.
- Needs `tesseract-ocr` in the cell template (`agent-runtime/templates/node22-agent`, rebuild it once with
  `scripts/merion-bake-node22-agent.sh`). There is no bundled sample, so `--test` is not available.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence
class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that
([VENDORS.md](../../../docs/merion/VENDORS.md)).
