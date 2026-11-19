import { test, expect } from '@playwright/test';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { replace } from '../astro/build.js';

// A figure a site keeps in its repo is only replaced once its new build is in hand, and a failure
// putting that in place leaves the kept one as it was. This runs in Node, once.
test.describe('replacing a kept figure', () => {
  let top;
  test.beforeEach(async ({}, testInfo) => {
    test.skip(testInfo.project.name !== 'desktop-chromium', 'Node only: once is enough');
    top = await mkdtemp(path.join(tmpdir(), 'pencil-kept-'));
    await mkdir(path.join(top, 'kept', 'circle-abc'), { recursive: true });
    await writeFile(path.join(top, 'kept', 'circle-abc', 'circle.json'), 'kept');
  });
  test.afterEach(() => top && rm(top, { recursive: true, force: true }));

  test('a failure putting the new one in place leaves the kept one', async () => {
    const dir = path.join(top, 'kept', 'circle-abc');
    // Not there to move, so the rename fails, and not because of another disk.
    await expect(replace(dir, path.join(top, 'missing'))).rejects.toThrow(/ENOENT/);
    expect(await readFile(path.join(dir, 'circle.json'), 'utf8')).toBe('kept');
    expect(await readdir(path.join(top, 'kept'))).toEqual(['circle-abc']);
  });

  test('a new build takes its place and nothing is left beside it', async () => {
    const dir = path.join(top, 'kept', 'circle-abc');
    const built = path.join(top, 'building');
    await mkdir(built);
    await writeFile(path.join(built, 'circle.json'), 'new');
    await replace(dir, built);
    expect(await readFile(path.join(dir, 'circle.json'), 'utf8')).toBe('new');
    expect(await readdir(path.join(top, 'kept'))).toEqual(['circle-abc']);
    expect(await readdir(top)).not.toContain('building');
  });
});
