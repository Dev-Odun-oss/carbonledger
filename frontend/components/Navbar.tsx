'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useWallet } from '@/lib/wallet/WalletContext';
import { useEffect, useState } from 'react';

/** Top-level nav items. `href` is also used as the prefix for nested-route matching. */
const NAV_LINKS = [
  { href: '/marketplace', label: 'Marketplace' },
  { href: '/projects',    label: 'Projects' },
  { href: '/audit',       label: 'Audit' },
  { href: '/retire',      label: 'Retire' },
  { href: '/dashboard',   label: 'Dashboard' },
] as const;

/**
 * Returns true when the current pathname matches a nav item.
 * Exact match for '/', prefix match for everything else so that
 * e.g. /marketplace/abc still highlights the Marketplace link.
 */
export function isActive(pathname: string, href: string): boolean {
  if (href === '/') return pathname === '/';
  return pathname === href || pathname.startsWith(href + '/');
}

export default function Navbar() {
  const pathname = usePathname();
  const { isConnected, publicKey, error, connect, disconnect, checkNetwork } = useWallet();
  const [networkWarning, setNetworkWarning] = useState<string | null>(null);

  useEffect(() => {
    if (isConnected) {
      checkNetwork().then(({ isCorrect, currentNetwork }) => {
        if (!isCorrect) {
          setNetworkWarning(`Network mismatch: ${currentNetwork}`);
        } else {
          setNetworkWarning(null);
        }
      });
    }
  }, [isConnected, checkNetwork]);

  const handleConnect = async () => {
    const result = await connect();
    if (!result.success && result.error?.includes('not installed')) {
      if (confirm('Freighter wallet not installed. Install now?')) {
        window.open('https://freighter.app/', '_blank');
      }
    }
  };

  const truncateAddress = (addr: string) => {
    if (!addr) return '';
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
  };

  return (
    <nav style={styles.nav} aria-label="Main navigation">
      {/* Logo */}
      <Link href="/" style={styles.logoLink}>CarbonLedger</Link>

      {/* Primary nav links */}
      <ul style={styles.navList} role="list">
        {NAV_LINKS.map(({ href, label }) => {
          const active = isActive(pathname, href);
          return (
            <li key={href}>
              <Link
                href={href}
                className={active ? 'nav-link nav-link--active' : 'nav-link'}
                aria-current={active ? 'page' : undefined}
              >
                {label}
              </Link>
            </li>
          );
        })}
      </ul>

      {/* Wallet controls */}
      <div style={styles.right}>
        {error && <span style={styles.error}>{error}</span>}
        {networkWarning && <span style={styles.warning}>{networkWarning}</span>}

        {isConnected && publicKey ? (
          <div style={styles.walletInfo}>
            <span style={styles.address}>{truncateAddress(publicKey)}</span>
            <button onClick={disconnect} style={styles.disconnectBtn}>
              Disconnect
            </button>
          </div>
        ) : (
          <button onClick={handleConnect} style={styles.connectBtn}>
            Connect Wallet
          </button>
        )}
      </div>
    </nav>
  );
}

const styles = {
  nav: {
    display: 'flex',
    alignItems: 'center',
    gap: '1rem',
    padding: '0 2rem',
    backgroundColor: '#1a1a2e',
    color: '#fff',
    flexWrap: 'wrap' as const,
    minHeight: '64px',
  },
  logoLink: {
    fontSize: '1.25rem',
    fontWeight: 'bold' as const,
    color: '#fff',
    textDecoration: 'none',
    marginRight: '1rem',
    /* min-height/padding come from the global .nav-link rule */
  },
  navList: {
    display: 'flex',
    alignItems: 'center',
    gap: '0.25rem',
    listStyle: 'none',
    margin: 0,
    padding: 0,
    flex: 1,
  },
  right: {
    display: 'flex',
    alignItems: 'center',
    gap: '1rem',
    flexWrap: 'wrap' as const,
    marginLeft: 'auto',
  },
  connectBtn: {
    padding: '0.5rem 1rem',
    backgroundColor: '#4CAF50',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
  },
  disconnectBtn: {
    padding: '0.5rem 1rem',
    backgroundColor: '#dc3545',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
  },
  walletInfo: {
    display: 'flex',
    alignItems: 'center',
    gap: '0.5rem',
  },
  address: {
    padding: '0.5rem 1rem',
    backgroundColor: '#16213e',
    borderRadius: '4px',
    fontFamily: 'monospace',
  },
  error: {
    color: '#ff6b6b',
    fontSize: '0.875rem',
  },
  warning: {
    color: '#ffc107',
    fontSize: '0.875rem',
  },
};