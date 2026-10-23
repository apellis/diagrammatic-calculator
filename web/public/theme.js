// The chosen theme before the first paint (src/ui/theme.ts keeps it). A file of its own, not
// inline, so the Content-Security-Policy can allow scripts from this site only.
try {
  const t = localStorage.getItem('diagcalc.theme');
  if (t === 'light' || t === 'dark') document.documentElement.dataset.theme = t;
} catch {
  // no storage: the system theme applies
}
