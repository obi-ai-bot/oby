// The web pages the boards show inside the browser, in fixtures mode (in live mode the page is
// agent-browser's own stream). A site's own look, outside Oby's (browser.css, .site-*).
import type { ReactNode } from 'react';
import { Lock } from 'lucide-react';
import { useLab } from './store';

const Top = ({ logo, name, links, signin }: { logo?: string; name: string; links?: ReactNode; signin?: string }) => (
  <div className="site-top">
    <span className="site-logo"><i style={logo ? { background: logo } : undefined} />{name}</span>
    {links && <span className="site-links">{links}</span>}
    {signin && <span className="site-signin">{signin}</span>}
  </div>
);
const On = ({ children }: { children: ReactNode }) => <span style={{ color: '#1a1b1f', fontWeight: 600 }}>{children}</span>;

export function SignIn({ focus = false }: { focus?: boolean }) {
  return (
    <div className="site">
      <Top name="Acme" links={<><span>Product</span><span>Pricing</span><span>Docs</span></>} />
      <div className="site-card">
        <h3>Welcome back</h3>
        <p>Sign in to your Acme workspace.</p>
        <span className="site-label">Email</span>
        <div className="site-input">test+0419@example.com</div>
        <span className="site-label">Password</span>
        <div className={`site-input${focus ? ' is-focus' : ''}`}>••••••••••••</div>
        <div className="site-button">Sign in</div>
        <div className="site-muted">No account? Sign up</div>
      </div>
    </div>
  );
}

export function SignUp() {
  return (
    <div className="site">
      <Top name="Acme" />
      <div className="site-card" style={{ marginTop: 26 }}>
        <h3>Create your account</h3>
        <p>Free for 14 days. No card needed.</p>
        <span className="site-label">Email</span>
        <div className="site-input is-focus">test+0419@exam<span style={{ width: 1.5, height: 16, background: '#1a1b1f' }} /></div>
        <span className="site-label">Password</span>
        <div className="site-input is-empty">At least 12 characters</div>
        <div className="site-button">Create account</div>
      </div>
    </div>
  );
}

export function Members() {
  return (
    <div className="site">
      <Top name="Acme" links={<><span>Projects</span><On>Members</On><span>Billing</span></>} signin="Invite" />
      <div className="site-hero"><h2>Members</h2><p>Everyone in the Acme workspace.</p></div>
      <div className="site-rows">
        <div className="site-row"><span>Name</span><span style={{ width: 120 }}>Role</span><span style={{ width: 90 }}>Status</span></div>
        <div className="site-row"><span>Test User 0419</span><span style={{ width: 120, color: '#6b6f7b' }}>Owner</span><span style={{ width: 90 }}><span className="pill-ok">Active</span></span></div>
        <div className="site-row"><span>Sam Example</span><span style={{ width: 120, color: '#6b6f7b' }}>Member</span><span style={{ width: 90 }}><span className="pill-ok">Active</span></span></div>
        <div className="site-row"><span>Robin Sample</span><span style={{ width: 120, color: '#6b6f7b' }}>Member</span><span style={{ width: 90 }}><span className="pill-bad">Invite expired</span></span></div>
      </div>
    </div>
  );
}

export function MembersScript() {
  const row = (name: string, bad: boolean) => (
    <div className={`site-row${bad ? ' is-hit' : ''}`}><span>{name}</span><span style={{ width: 120 }}><span className={bad ? 'pill-bad' : 'pill-ok'}>{bad ? 'Invite expired' : 'Active'}</span></span></div>
  );
  return (
    <div className="site">
      <Top name="Acme" links={<><span>Projects</span><On>Members</On></>} />
      <div className="site-hero"><h2>Members</h2><p>Everyone in the Acme workspace.</p></div>
      <div className="site-rows">
        <div className="site-row"><span>Name</span><span style={{ width: 120 }}>Status</span></div>
        {row('Test User 0419', false)}{row('Robin Sample', true)}{row('Sam Example', false)}{row('Kai Placeholder', true)}{row('Lee Demo', true)}
      </div>
    </div>
  );
}

/** The staging admin's sign-in (7, 8): its Password field ringed while Oby's card fills it. */
export function AdminSignIn({ tab }: { tab: string }) {
  const { ui } = useLab();
  const filling = ui.card === 'password';
  const filled = ui.filled?.tab === tab && ui.filled.field === 'Password' ? ui.filled.length : 0;
  return (
    <div className="site">
      <Top logo="#111318" name="Acme Admin" links={<span>Staging</span>} />
      <div className="site-card">
        <h3>Sign in</h3>
        <p>Staff only. Use your Acme account.</p>
        <span className="site-label">Email</span>
        <div className="site-input">admin@example.com</div>
        <span className="site-label">Password</span>
        {filling || filled
          ? <div className="site-input is-oby" data-lab="page-password" style={filled ? undefined : { color: '#9a9ea9' }}>{filled ? '•'.repeat(filled) : 'Password'}<span className="fill-badge"><Lock size={10} strokeWidth={1.6} />From Oby</span></div>
          : <div className="site-input is-empty" data-lab="page-password">Password</div>}
        <div className="site-button" style={{ background: '#111318' }}>Sign in</div>
        <div className="or">OR</div>
        <div className="sso">Continue with SSO</div>
      </div>
    </div>
  );
}

export function Pricing() {
  return (
    <div className="docs">
      <div className="docs-nav"><b>Acme</b><span>Overview</span><span>Features</span><On>Pricing</On><span>Customers</span><span>Changelog</span></div>
      <div className="docs-main">
        <h2>Pricing</h2>
        <div style={{ color: '#6b6f7b', fontSize: 13 }}>Simple plans that grow with your team.</div>
        <div className="plans">
          <div className="plan"><b>Starter</b><span className="price">$0</span><i /><i style={{ width: '70%' }} /></div>
          <div className="plan is-read"><b>Team</b><span className="price">$12</span><i /><i style={{ width: '60%' }} /></div>
          <div className="plan"><b>Business</b><span className="price">$29</span><i /><i style={{ width: '80%' }} /></div>
        </div>
        {[80, 64, 72].map((w, i) => <div key={i} style={{ marginTop: i ? 10 : 22, height: 8, width: `${w}%`, borderRadius: 4, background: '#eef0f4' }} />)}
      </div>
    </div>
  );
}

export function Invoices() {
  const row = (n: string, amt: string, ok: boolean) => (
    <div className="site-row"><span>{n}</span><span style={{ width: 90 }}>{amt}</span><span style={{ width: 90 }}><span className={ok ? 'pill-ok' : 'pill-bad'}>{ok ? 'Paid' : 'Failed'}</span></span></div>
  );
  return (
    <div className="site">
      <Top logo="#0e9f6e" name="Billing" links={<><On>Invoices</On><span>Statements</span></>} signin="Export CSV" />
      <div className="site-hero"><h2>Invoices</h2><p>October 2026</p></div>
      <div className="site-rows">
        <div className="site-row"><span>Invoice</span><span style={{ width: 90 }}>Amount</span><span style={{ width: 90 }}>Status</span></div>
        {row('INV-1042', '$240.00', false)}{row('INV-1049', '$96.00', true)}{row('INV-1057', '$1,180.00', false)}
      </div>
    </div>
  );
}

export function Dashboard() {
  return (
    <div className="dash">
      <div className="dash-top"><i />Acme · Dashboard<span style={{ marginLeft: 'auto', color: '#8b93a3', fontWeight: 400, fontSize: 12 }}>dev build</span></div>
      <div className="dash-grid">
        <div className="kpi"><span>Active users</span><b>1,284</b></div>
        <div className="kpi"><span>Sign-ups today</span><b>37</b></div>
        <div className="kpi"><span>Errors</span><b>0</b></div>
      </div>
      <div className="chart"><svg viewBox="0 0 560 150" preserveAspectRatio="none" aria-hidden="true"><polyline points="0,120 60,110 120,112 180,90 240,96 300,70 360,78 420,52 480,60 560,38" fill="none" stroke="#22c55e" strokeWidth="2" /></svg></div>
    </div>
  );
}

export const Bone = ({ w, top, h }: { w: string; top?: number; h?: number }) => <span className="bone" style={{ width: w, marginTop: top, height: h }} />;

export function Settings() {
  return <div className="site"><Top name="Acme" /><Bone w="50%" top={20} h={14} /><Bone w="70%" /><Bone w="64%" /></div>;
}
