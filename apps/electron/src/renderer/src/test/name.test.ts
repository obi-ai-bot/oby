import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { menu } from '../../../shared/menu';

const file = (path: string) => readFileSync(new URL(`../../../../${path}`, import.meta.url), 'utf8');
const builder = file('electron-builder.yml');

// The app is Oby to people (#1853), and everything an install already has keeps its old name.
describe('the name', () => {
  it('people_see_oby_in_the_bundle_and_the_menu', () => {
    expect(builder).toMatch(/^productName: Oby$/m);
    const app = menu[0].submenu as { role?: string; label?: string }[];
    expect(menu[0].label).toBe('Oby');
    // the roles that say app.name unless told otherwise
    for (const [role, label] of [['about', 'About Oby'], ['hide', 'Hide Oby'], ['quit', 'Quit Oby']])
      expect(app.find((i) => i.role === role)?.label).toBe(label);
    expect(file('src/main/index.ts')).toContain('Menu.setApplicationMenu(Menu.buildFromTemplate(menu))');
  });

  it('the_keychain_item_and_the_apps_folder_stay_named_oby_app', () => {
    // app.name is package.json's productName, else its name, and names both (Electron's init.ts,
    // electron_browser_main_parts.cc: "<app.name> Safe Storage")
    const pkg = JSON.parse(file('package.json'));
    expect(pkg.name).toBe('oby-app');
    expect(pkg.productName).toBeUndefined();
    // the two other ways to change it: electron-builder writing into the bundle's package.json, or main
    expect(builder).not.toMatch(/^extraMetadata:/m);
    for (const f of readdirSync(new URL('../../../main/', import.meta.url)))
      expect(file(`src/main/${f}`), f).not.toMatch(/app\.(name\s*=[^=]|setName\()/);
  });

  it('the_bundle_id_and_the_engines_label_stay', () => {
    expect(builder).toMatch(/^appId: com\.deskree\.oby\.electron$/m);
    const plist = file('build/com.deskree.oby.electron.obid.plist');
    expect(plist).toContain('<string>com.deskree.oby.electron.obid</string>');
    expect(plist).toContain('<string>Contents/Helpers/obid</string>');
  });
});
