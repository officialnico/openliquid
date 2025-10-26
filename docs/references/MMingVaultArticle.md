Decompiling Hyperliquid's MMing Vault
Hyperliquid's Market Making Vault has been crucial to its success. It onboarded liquidity while providing scaling ability, solving a fundamental DeFi dilemma.
Before Hyperliquid, DeFi protocols chose between an easy bootstrap vs. scaling. Mechanisms like GMX, vAMMs, and k=XY, which chose initial onboarding, took off at a big cost to LPs and eventual scaling. While those who chose scale never got the initial traction and never scaled.
Hyperliquid solved this problem by allowing complex strats to be expressed in the same place as its vault. It has achieved such a great result and onboarded so many makers that the protocol doesn't even need the vault anymore.
Yet the mechanism behind how the vault operates is barely understood. To change that, I scraped 12 hours of all Hyperliquid vault orders and positions.
Vault Implementation
Hyperliquid runs two market-making vaults. Both use the same strategy but with different hyperparameters. These vaults hedge with each other and function as a single unit. So, for my analysis, I combined their orders and positions. For example, if vault A has a -$10k BTC position while vault B has $7k BTC, I treat this as a net -$3k position.
The Vault dynamically tiers pairs
Each coin on Hyperliquid gets assigned to a tier. The tier defines the amount of liquidity the vault quotes at the top of the book on both the buy and sell sides.
Table 1: Hyperliquid Best Quote Tier
The biggest pairs, BTC and ETH, get 0.1% of the individual vault liquidity as their best order size, followed by 0.05% for XRP and SOL, followed by another tier for SUI, LINK, BNB, AVAX, and others. Most coins lie in tier 4 or 5 (~80 each).
The Vault quotes both sides at a spread from the true price
Hyperliquid's initial blog mentions that it quotes at a spread from fair price. Often this fair price calculation is a part of an MMing firm's alpha and is computed using variables like book depth, imbalance, volatility, and such across multiple exchanges.
Most people assume that there is a lot of alpha in this price and thus think that creating a protocol-level vault is futile. To verify if this is the case, I ran an analysis.
Hyperliquid's vault changes the size it is quoting, making it either bid heavy or ask heavy when it feels confident about a short-term directional change. I grouped coins by pair and measured the accuracy of its confidence.
Coinwise accuracy of Hyperliquid's MM Vault prediction
The accuracy of its prediction ranges from 46% to 55%, with an average of 49.8%. BTC and ETH had an accuracy close to the median at 49.9% and 50.2%.
This is close to a coin toss. If you have trained short-term regressions, you will know that there is nothing groundbreaking about 50% accuracy for a 10s price prediction.
And that's exactly how it should be. Protocol vaults are judged by their worst day, not their best. You don't want to be too clever here - you want to break even and provide a good trading experience without breaking anything. This coin-toss-level accuracy accomplishes that goal. And it allows the vault to maintain reasonable spread ~0.1% for most of its pairs (BTC has around ~0.065%, while some tail pairs have over 0.5%)
The quote size is gradually affected by its positional exposure
There are tiers of maximum exposure the vault is willing to take for a coin. It's around ~0.33% for a few coins and ranges down to 0.08% for others.
As the coin starts approaching this number, it starts quoting less in that direction.
BTC positions vs. net order liquidity (bid - ask)
HYPE positions vs. net order liquidity (bid - ask)
You can see in the charts above that the vault is willing to quote a large amount for most of the range, but as its exposure increases, it slowly starts quoting less in that direction.
Conclusion
The Hyperliquid Market Making Vault uses a fairly simple algorithm with specific conditions and a reasonable accuracy to make its book. This mechanism has made its LPs $62M in profit and easily outperforms Uniswap or GLP LPs.
Yet, everyone talks about the Uniswap mechanism, while no one talks about the Hyperliquid mechanism. The reason for this disconnect is that most people designing protocols are software engineers who have no idea about quant strategies but are willing to spend years optimizing a curve that the LPs will never profit from. The marketers and VCs enable this because none of them have a clue either.
If DeFi is to dominate CeFi liquidity, it needs more HLP-style conditional vaults, which work like a quant maker, and fewer Uniswap curves and GLPs which resign protocols to a second or third-tier status in price discovery.