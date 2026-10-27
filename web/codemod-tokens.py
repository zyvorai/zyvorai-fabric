# Maps literal-hex Tailwind utilities and bg-white onto the --zf-* tokens (ux-plan step 2).
# The mapping is exactly what main.css's dark-mode redirect block did, so light and dark look the same.
# Opacity variants (bg-[#0066cc]/20) become bg-[var(--zf-link)]/20, so in dark mode their tint follows the token.
import re, sys, pathlib

TOK = {
    'text-[#1d1d1f]': 'text-[var(--zf-ink)]',
    'text-[#6e6e73]': 'text-[var(--zf-muted)]',
    'placeholder-[#6e6e73]': 'placeholder-[var(--zf-muted)]',
    'border-[#d2d2d7]': 'border-[var(--zf-hairline)]',
    'divide-[#d2d2d7]': 'divide-[var(--zf-hairline)]',
    'bg-[#d2d2d7]': 'bg-[var(--zf-hairline)]',
    'bg-[#f5f5f7]': 'bg-[var(--zf-canvas)]',
    'bg-[#e8e8ed]': 'bg-[var(--zf-canvas-alt)]',
    'border-[#e8e8ed]': 'border-[var(--zf-canvas-alt)]',
    'bg-[#0066cc]': 'bg-[var(--zf-link)]',
    'text-[#0066cc]': 'text-[var(--zf-link)]',
    'border-[#0066cc]': 'border-[var(--zf-link)]',
    'bg-[#0077ed]': 'bg-[var(--zf-link-hover)]',
    'bg-[#0071e3]': 'bg-[var(--zf-link)]',
    'text-[#0071e3]': 'text-[var(--zf-link)]',
}
pat = re.compile(r'(?<![\w\[-])(' + '|'.join(re.escape(k) for k in TOK) + r')(?![\w])', re.I)
white = re.compile(r'(?<![\w-])bg-white(?![\w/-])')
n = 0
for f in pathlib.Path('src').rglob('*.ts*'):
    s = f.read_text()
    t = pat.sub(lambda m: TOK[m.group(1).lower()], s)
    t = white.sub('bg-[var(--zf-surface)]', t)
    if t != s:
        n += 1
        f.write_text(t)
print('files changed', n)
