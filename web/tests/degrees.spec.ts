import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, setContext, typeCell } from './helpers';

test('cyclotomic watch and solve accept products above the proven vanishing degree', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klr type=A2 nu=2i+j ring=QQ Lambda=2i+j');
  await typeCell(page, 'def e := c1 x_2 x_3 e(iji) + c2 psi_1 x_2 psi_2 psi_1 e(iji)');
  await addCell(page, 'watch e e - e');
  await addCell(page, 'solve e e - e = 0 for c1, c2');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText(/^(0|\d+ terms?, degrees? .+)$/);
  await expect(cell(page, 2).getByTestId('summary')).toHaveText(/solution|family/);
  await expect(page.locator('.cell .error')).toHaveCount(0);
  await page.reload();
  await expect(cell(page, 1).getByTestId('summary')).toHaveText(/^(0|\d+ terms?, degrees? .+)$/);
  await expect(cell(page, 2).getByTestId('summary')).toHaveText(/solution|family/);
});

test('high-degree nonzero values survive while known-zero quotient terms vanish', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klr type=A1 nu=i ring=QQ Lambda=11i');
  await typeCell(page, 'watch x_1^20 + x_1^10');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 20');
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', 'x_1^10');
  await addCell(page, 'check zero(x_1^20)');
  await expect(cell(page, 1).getByTestId('verdict')).toHaveText('holds');
});
