# Gives accessible names to controls the axe gate cannot see (it only loads empty pages):
#  - icon-only <button>s get aria-label + title from the icon's name
#  - an <input>/<select>/<textarea> directly after a plain-text <label> gets aria-label with that text
# Run from web/: python3 codemod-a11y-names.py [--dry]
import re, pathlib, sys, collections

ICONS = {
    'Trash2': 'Delete', 'Trash': 'Delete', 'X': 'Close', 'XCircle': 'Close', 'Edit': 'Edit', 'Edit2': 'Edit', 'Edit3': 'Edit',
    'Pencil': 'Edit', 'Plus': 'Add', 'PlusCircle': 'Add', 'RefreshCw': 'Refresh', 'RotateCw': 'Refresh', 'Copy': 'Copy',
    'Download': 'Download', 'Upload': 'Upload', 'Play': 'Start', 'Square': 'Stop', 'Pause': 'Pause', 'Eye': 'Show',
    'EyeOff': 'Hide', 'Settings': 'Settings', 'Search': 'Search', 'ChevronDown': 'Expand', 'ChevronUp': 'Collapse',
    'ChevronRight': 'Expand', 'ChevronLeft': 'Back', 'ArrowLeft': 'Back', 'ExternalLink': 'Open in new tab', 'Save': 'Save',
    'Check': 'Confirm', 'Star': 'Favorite', 'MoreHorizontal': 'More actions', 'MoreVertical': 'More actions',
    'Power': 'Power', 'RotateCcw': 'Undo', 'Undo': 'Undo', 'Link': 'Link', 'Key': 'Key', 'Terminal': 'Terminal',
    'Camera': 'Snapshot', 'Maximize2': 'Fullscreen', 'Minimize2': 'Exit fullscreen', 'Filter': 'Filter', 'Info': 'More info',
}
dry = '--dry' in sys.argv
btn = re.compile(r'<button\b((?:[^>{]|\{[^}]*\})*)>(\s*)(<([A-Z][A-Za-z0-9]*)\b[^>]*/>)(\s*)</button>', re.S)
lab = re.compile(r'(<label\b[^>{}]*>)\s*([A-Za-z0-9][^<>{}]*?)\s*(</label>)(\s*)<(input|select|textarea)\b((?:[^>{]|\{(?:[^{}]|\{[^}]*\})*\})*?)(/?>)', re.S)
bn = ln = 0
unmapped = collections.Counter()
for f in pathlib.Path('src').rglob('*.tsx'):
    t = f.read_text()
    def fix_btn(m):
        global bn
        attrs, icon = m.group(1), m.group(4)
        if 'aria-label' in attrs or 'title=' in attrs: return m.group(0)
        name = ICONS.get(icon)
        if not name:
            unmapped[icon] += 1
            return m.group(0)
        bn += 1
        return f'<button{attrs.rstrip()} aria-label="{name}" title="{name}">{m.group(2)}{m.group(3)}{m.group(5)}</button>' if not attrs.endswith('\n') else m.group(0)
    def fix_lab(m):
        global ln
        rest = m.group(6)
        if re.search(r'aria-label|aria-labelledby|\bid=', rest) or re.search(r'type=["\'](hidden|checkbox|radio|file|range)', rest): return m.group(0)
        text = re.sub(r'\s+', ' ', m.group(2)).strip().rstrip(':').replace('"', "'")
        if not text or len(text) > 60: return m.group(0)
        ln += 1
        return f'{m.group(1)}{m.group(2)}{m.group(3)}{m.group(4)}<{m.group(5)} aria-label="{text}"{rest}{m.group(7)}'
    u = lab.sub(fix_lab, btn.sub(fix_btn, t))
    if u != t and not dry: f.write_text(u)
print('buttons named:', bn, ' fields named:', ln, ' unmapped icons:', dict(unmapped))
