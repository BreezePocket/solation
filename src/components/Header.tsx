import React from 'react';
import { Link, useLocation } from 'react-router-dom';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';

export const Header: React.FC = () => {
  const location = useLocation();

  const isActive = (path: string) => {
    return location.pathname === path;
  };

  return (
    <header className="bg-white border-b-2 border-retro-black sticky top-0 z-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex justify-between items-center h-16">
          {/* Left: Navigation */}
          <nav className="flex items-center space-x-8">
            <Link to="/" className="text-xl font-bold text-retro-green">
              BREEZEPOCKET
            </Link>

            <div className="flex space-x-6">
              <Link
                to="/earn"
                className={`font-medium transition-colors ${isActive('/earn') || isActive('/') || location.pathname.startsWith('/trade')
                    ? 'text-retro-green'
                    : 'text-retro-gray-600 hover:text-retro-black'
                  }`}
              >
                Earn
              </Link>
              <Link
                to="/dashboard"
                className={`font-medium transition-colors ${isActive('/dashboard')
                    ? 'text-retro-green'
                    : 'text-retro-gray-600 hover:text-retro-black'
                  }`}
              >
                Dashboard
              </Link>
            </div>
          </nav>

          {/* Right: Wallet Connect Button */}
          <div className="flex items-center">
            <WalletMultiButton className="!bg-white hover:!bg-retro-gray-100 !text-retro-black !border-2 !border-retro-black !rounded-retro !font-mono !shadow-retro hover:!shadow-retro-lg !transition-all !duration-200 hover:!-translate-y-0.5" />
          </div>
        </div>
      </div>
    </header>
  );
};
