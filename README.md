# BreezePocket - Earn Upfront Premiums on Your Crypto

A beautiful, retro-styled decentralized finance (DeFi) options trading interface built with React, TypeScript, and Solana Wallet Adapter.

## Features

- **Retro UI Design**: Beautiful brutalist design with beige backgrounds, green accents, and monospaced typography
- **Solana Integration**: Seamless wallet connectivity with Phantom, Solflare, and other Solana wallets
- **Options Trading**: Support for Covered Call and Cash Secured Put strategies
- **Multiple Assets**: Trade options on SOL, JUPSOL, JITOSOL, cbBTC, xBTC, and ETH
- **Real-time Balance**: Fetch and display user wallet balances
- **Dashboard**: Track positions, earnings, and income over time
- **Responsive Design**: Works beautifully on desktop, tablet, and mobile

## Tech Stack

- **React 18** with TypeScript
- **Vite** for fast development and builds
- **Tailwind CSS** for styling with custom retro theme
- **Solana Wallet Adapter** for wallet connectivity
- **React Router** for navigation
- **Recharts** for data visualization

## Getting Started

### Prerequisites

- Node.js 16+ and npm
- A Solana wallet (Phantom, Solflare, etc.)

### Installation

1. Install dependencies:
```bash
npm install
```

2. Start the development server:
```bash
npm run dev
```

3. Open your browser to `http://localhost:5173`

### Building for Production

```bash
npm run build
```

## Project Structure

```
breezepocket/
├── src/
│   ├── components/      # Reusable components
│   │   └── Header.tsx   # Navigation header with wallet connect
│   ├── pages/           # Page components
│   │   ├── EarnPage.tsx        # Asset selection and strategy tabs
│   │   ├── TradingPage.tsx     # Trading interface with breadcrumbs
│   │   └── DashboardPage.tsx   # Positions and metrics dashboard
│   ├── context/         # React context providers
│   │   └── WalletContextProvider.tsx
│   ├── types/           # TypeScript type definitions
│   │   └── index.ts
│   ├── utils/           # Utility functions
│   │   └── assetConfig.ts
│   ├── App.tsx          # Main app component with routing
│   ├── main.tsx         # Entry point
│   └── index.css        # Global styles and Tailwind imports
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
└── tailwind.config.js
```

## Usage

### Connecting Your Wallet

1. Click "Connect" button in the top right
2. Select your Solana wallet from the modal
3. Approve the connection in your wallet

### Trading Options

1. Navigate to the "Earn" page
2. Select your strategy: Covered Call or Cash Secured Put
3. Click on an asset to open the trading interface
4. Use breadcrumbs to switch asset, strategy, or expiration date
5. Select a strike price and enter the amount
6. Click "Earn upfront premium now" to execute (simulated)

### Viewing Dashboard

1. Navigate to "Dashboard"
2. View your total income, active positions, and earnings
3. Analyze income trends with the chart
4. Review current positions in the table

## Customization

### Colors

Edit `tailwind.config.js` to customize the retro color palette:

```javascript
colors: {
  retro: {
    beige: '#F5F1E8',
    green: '#00D084',
    // ...
  }
}
```

### Assets

Add or modify assets in `src/utils/assetConfig.ts`:

```typescript
export const ASSETS: Record<AssetSymbol, Asset> = {
  // Add your asset here
}
```

## License

Business Source License 1.1 (BSL 1.1)

## Acknowledgments

- Design inspired by [Rysk Finance](https://app.rysk.finance/)
- Built with [Solana Wallet Adapter](https://github.com/solana-labs/wallet-adapter)
