import { expect, type Page } from '@playwright/test';

/** Opens a new empty document (KLR, type A2, ν = 2i+j) and returns once its first cell editor has the focus. */
export async function newDoc(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'New document' }).click();
  await page.getByRole('dialog', { name: 'New document' }).getByRole('button', { name: 'Create' }).click();
  // focused, not only shown: keys typed before the editor takes the focus would be lost
  await expect(page.locator('.cm-content')).toBeFocused();
}

/**
 * Sets the document's context with the chip and waits until the chip shows it, then puts the
 * cursor back in the new cell's editor (the engine checks the line before it is set, so typing
 * straight away would race it).
 */
export async function setContext(page: Page, line: string) {
  await page.getByTestId('context').click();
  await page.getByLabel('Context').fill(line);
  await page.getByLabel('Context').press('Enter');
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', new RegExp(`^${line.split(' ')[0]}\\b`));
  await page.locator('.cm-content').click();
}

/** Types a cell into the open editor of a new cell and commits it. */
export async function typeCell(page: Page, text: string) {
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

/** Adds a cell at the end. */
export async function addCell(page: Page, text: string) {
  await page.getByTestId('add-cell').click();
  await typeCell(page, text);
}

export const cell = (page: Page, n: number) => page.locator('.cell[data-cell]:not([data-cell="new"])').nth(n);

/**
 * Milliseconds from the person's input (the first `input` event that `act` causes: a click or a
 * key) until the summary of cell `n` reads `want`, measured in the page with the event's own time
 * stamp and a mutation observer, so neither Playwright's round trips nor its polling count.
 */
export async function timeUntil(page: Page, n: number, want: string, input: 'click' | 'keydown', act: () => Promise<void>): Promise<number> {
  await page.evaluate(([n, want, input]) => {
    const w = window as unknown as { __t: Promise<number> };
    const el = () => document.querySelectorAll('.cell[data-cell]:not([data-cell="new"])')[n as number]?.querySelector('[data-testid=summary]');
    let start: number | null = null;
    const onInput = (e: Event) => {
      if (start === null && e.isTrusted) start = e.timeStamp;
    };
    document.addEventListener(input as string, onInput, { capture: true });
    w.__t = new Promise((resolve, reject) => {
      const check = () => {
        if (el()?.textContent !== want) return;
        obs.disconnect();
        document.removeEventListener(input as string, onInput, { capture: true });
        if (start === null) reject(new Error(`the summary changed before any ${input}`));
        else resolve(performance.now() - start);
      };
      const obs = new MutationObserver(check);
      obs.observe(document.body, { subtree: true, childList: true, characterData: true });
    });
  }, [n, want, input] as const);
  await act();
  return page.evaluate(() => (window as unknown as { __t: Promise<number> }).__t);
}
