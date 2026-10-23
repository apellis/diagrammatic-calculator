import { expect, type Page } from '@playwright/test';

/** Opens a new empty document (KLR, type A2, ν = 2i+j) and returns once its first cell editor is ready. */
export async function newDoc(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'New document' }).click();
  await page.getByRole('dialog', { name: 'New document' }).getByRole('button', { name: 'Create' }).click();
  await expect(page.locator('.cm-content')).toBeVisible();
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
 * Milliseconds from just before `act` until the summary of cell `n` reads `want`, measured in the
 * page with a mutation observer (so Playwright's polling does not count).
 */
export async function timeUntil(page: Page, n: number, want: string, act: () => Promise<void>): Promise<number> {
  await page.evaluate(([n, want]) => {
    const w = window as unknown as { __t: Promise<number> };
    const el = () => document.querySelectorAll('.cell[data-cell]:not([data-cell="new"])')[n as number]?.querySelector('[data-testid=summary]');
    const start = performance.now();
    w.__t = new Promise((resolve) => {
      const check = () => {
        if (el()?.textContent === want) {
          obs.disconnect();
          resolve(performance.now() - start);
        }
      };
      const obs = new MutationObserver(check);
      obs.observe(document.body, { subtree: true, childList: true, characterData: true });
    });
  }, [n, want] as const);
  await act();
  return page.evaluate(() => (window as unknown as { __t: Promise<number> }).__t);
}
