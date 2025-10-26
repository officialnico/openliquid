# High-frequency trading in a limit order book 

Marco Avellaneda \& Sasha Stoikov

October 5, 2006


#### Abstract

We study a stock dealer's strategy for submitting bid and ask quotes in a limit order book. The agent faces an inventory risk due to the diffusive nature of the stock's mid-price and a transactions risk due to a Poisson arrival of market buy and sell orders. After setting up the agent's problem in a maximal expected utility framework, we derive the solution in a two step procedure. First, the dealer computes a personal indifference valuation for the stock, given his current inventory. Second, he calibrates his bid and ask quotes to the market's limit order book. We compare this "inventory-based" strategy to a "naive" strategy that is symmetric around the mid-price, by simulating stock price paths and displaying the P\&L profiles of both strategies. We find that our strategy yields P\&L profiles and final inventories that have significantly less variance than the benchmark strategy.


## 1 Introduction

The role of a dealer in securities markets is to provide liquidity on the exchange by quoting bid and ask prices at which he is willing to buy and sell a specific quantity of assets. Traditionally, this role has been filled by market-maker or specialist firms. In recent years, with the growth of electronic exchanges such as Nasdaq's Inet, anyone willing to submit limit orders in the system can effectively play the role of a dealer. Indeed, the availability of high frequency data on the limit order book (see www.inetats.com) ensures a fair playing field where various agents can post limit orders at the prices they choose. In this paper, we study the optimal submission strategies of bid and ask orders in such a limit order book.

The pricing strategies of dealers have been studied extensively in the microstructure literature. The two most often addressed sources of risk facing the dealer are (i) the inventory risk arising from uncertainty in the asset's value and (ii) the asymmetric information risk arising from informed traders. Useful surveys of their results can be found in Biais et al. [1], Stoll [13] and a book by O'Hara [10]. In this paper, we will focus on the inventory effect. In fact, our model is closely related to a paper
by Ho and Stoll [6], which analyses the optimal prices for a monopolistic dealer in a single stock. In their model, the authors specify a 'true' price for the asset, and derive optimal bid and ask quotes around this price, to account for the effect of the inventory. This inventory effect was found to be significant in an empirical study of AMEX Options by Ho and Macris [5]. In another paper by Ho and Stoll [7], the problem of dealers under competition is analyzed and the bid and ask prices are shown to be related to the reservation (or indifference) prices of the agents. In our framework, we will assume that our agent is but one player in the market and the 'true' price is given by the market mid-price.

Of crucial importance to us will be the arrival rate of buy and sell orders that will reach our agent. In order to model these arrival rates, we will draw on recent results in econophysics. One of the important achievements of this literature has been to explain the statistical properties of the limit order book (see Bouchaud et al. [2], Potters and Bouchaud [11], Smith et al. [12], Luckock [8]). The focus of these studies has been to reproduce the observed patterns in the markets by introducing 'zero intelligence' agents, rather than modeling optimal strategies of rational agents. One possible exception is the work of Luckock [8], who defines a notion of optimal strategies, without resorting to utility functions. Though our objective is different to that of the econophysics literature, we will draw on their results to infer reasonable arrival rates of buy and sell orders. In particular, the results that will be most useful to us are the size distribution of market orders (Gabaix et al. [3], Maslow and Mills [9]) and the temporary price impact of market orders (Weber and Rosenow [14], Bouchaud et al. [2]).

Our approach, therefore, is to combine the utility framework of the Ho and Stoll approach with the microstructure of actual limit order books as described in the econophysics literature. The main result is that the optimal bid and ask quotes are derived in an intuitive two-step procedure. First, the dealer computes a personal indifference valuation for the stock, given his current inventory. Second, he calibrates his bid and ask quotes to the limit order book, by considering the probability with which his quotes will be executed as a function of their distance from the mid-price. In the balancing act between the dealer's personal risk considerations and the market environment lies the essence of our solution.

The paper is organized as follows. In section 2, we describe the main building blocks for the model: the dynamics of the mid-market price, the agent's utility objective and the arrival rate of orders as a function of the distance to the mid-price. In section 3, we solve for the optimal bid and ask quotes, and relate them to the reservation price of the agent, given his current inventory. We then present an approximate solution, numerically simulate the performance of our agent's strategy and compare its Profit and Loss (P\&L) profile to that of a benchmark strategy.

## 2 The model

### 2.1 The mid-price of the stock

For simplicity, we assume that money market pays no interest. The mid-market price, or mid-price, of the stock evolves according to

$$
\begin{equation*}
d S_{u}=\sigma d W_{u} \tag{2.1}
\end{equation*}
$$

with initial value $S_{t}=s$. Here $W_{t}$ is a standard one-dimensional Brownian Motion and $\sigma$ is constant. ${ }^{1}$ Underlying this continuous-time model is the implicit assumption that our agent has no opinion on the drift or any autocorrelation structure for the stock.

This mid-price will be used solely to value the agent's assets at the end of the investment period. He may not trade costlessly at this price, but this source of randomness will allow us to measure the risk of his inventory in stock. In section 2.4 we will introduce the possibility to trade through limit orders.

### 2.2 The optimizing agent with finite horizon

The agent's objective is to maximize the expected exponential utility of his P\&L profile at a terminal time $T$. This choice of convex risk measure is particularly convenient, since it will allow us to define reservation (or indifference) prices which are independent of the agent's wealth.

We first model an inactive trader who does not have any limit orders in the market and simply holds an inventory of $q$ stocks until the terminal time $T$. This "frozen inventory" strategy will later prove to be useful in the case when limit orders are allowed. The agent's value function is

$$
v(x, s, q, t)=E_{t}\left[-\exp \left(-\gamma\left(x+q S_{T}\right)\right]\right.
$$

where $x$ is the initial wealth in dollars. This value function can be written as

$$
\begin{equation*}
v(x, s, q, t)=-\exp (-\gamma x) \exp (-\gamma q s) \exp \left(\frac{\gamma^{2} q^{2} \sigma^{2}(T-t)}{2}\right) \tag{2.3}
\end{equation*}
$$

which shows us directly its dependence on the market parameters.

[^0]We may now define the reservation bid and ask prices for the agent. The reservation bid price is the price that would make the agent indifferent between his current portfolio and his current portfolio plus one stock. The reservation ask price is defined similarly below. We stress that this is a subjective valuation from the point of view of the agent and does not reflect a price at which trading should occur.
Definition 1. Let $v$ be the value function of the agent. His reservation bid price $r^{b}$ is given implicitly by the relation

$$
\begin{equation*}
v\left(x-r^{b}(s, q, t), s, q+1, t\right)=v(x, s, q, t) \tag{2.4}
\end{equation*}
$$

The reservation ask price $r^{a}$ solves

$$
\begin{equation*}
v\left(x+r^{a}(s, q, t), s, q-1, t\right)=v(x, s, q, t) . \tag{2.5}
\end{equation*}
$$

A simple computation involving equations (2.3), (2.4) and (2.5) yields a closedform expression for the two prices

$$
\begin{equation*}
r^{a}(s, q, t)=s+(1-2 q) \frac{\gamma \sigma^{2}(T-t)}{2} \tag{2.6}
\end{equation*}
$$

and

$$
\begin{equation*}
r^{b}(s, q, t)=s+(-1-2 q) \frac{\gamma \sigma^{2}(T-t)}{2} \tag{2.7}
\end{equation*}
$$

in the setting where no trading is allowed. We will refer to the average of these two prices as the reservation or indifference price

$$
r(s, q, t)=s-q \gamma \sigma^{2}(T-t)
$$

given that the agent is holding $q$ stocks. This price is an adjustment to the midprice, which accounts for the inventory held by the agent. If the agent is long stock ( $q>0$ ), the reservation price is below the mid-price, indicating a desire to liquidate the inventory by selling stock. On the other hand, if the agent is short stock ( $q<0$ ), the reservation price is above the mid-price, since the agent is willing to buy stock at a higher price.

### 2.3 The optimizing agent with infinite horizon

Because of our choice of a terminal time $T$ at which we measure the performance of our agent, the reservation price (2.2) depends on the time interval $(T-t)$. Intuitively, the closer our agent is to time $T$, the less risky his inventory in stock is, since it can be liquidated at the mid-price $S_{T}$. In order to obtain a stationary version of the reservation price, we may consider an infinite horizon objective of the form

$$
\bar{v}(x, s, q)=E\left[\int_{0}^{\infty}-\exp (-\omega t) \exp \left(-\gamma\left(x+q S_{t}\right)\right) d t\right]
$$

The stationary reservation prices (defined in the same way as in Definition 1) are given by

$$
\bar{r}^{a}(s, q)=s+\frac{1}{\gamma} \ln \left(1+\frac{(1-2 q) \gamma^{2} \sigma^{2}}{2 \omega-\gamma^{2} q^{2} \sigma^{2}}\right)
$$

and

$$
\bar{r}^{b}(s, q)=s+\frac{1}{\gamma} \ln \left(1+\frac{(-1-2 q) \gamma^{2} \sigma^{2}}{2 \omega-\gamma^{2} q^{2} \sigma^{2}}\right)
$$

where $\omega>\frac{1}{2} \gamma^{2} \sigma^{2} q^{2}$.
The parameter $\omega$ may therefore be interpreted as an upper bound on the inventory position our agent is allowed to take. The natural choice of $\omega=\frac{1}{2} \gamma^{2} \sigma^{2}\left(q_{\text {max }}+1\right)^{2}$ would ensure that the prices defined above are bounded.

### 2.4 Limit orders

We now turn to an agent who can trade in the stock through limit orders that he sets around the mid-price given by (2.1). The agent quotes the bid price $p^{b}$ and the ask price $p^{a}$, and is committed to respectively buy and sell one share of stock at these prices, should he be "hit" or "lifted" by a market order. These limit orders $p^{b}$ and $p^{a}$ can be continuously updated at no cost. The distances

$$
\delta^{b}=s-p^{b}
$$

and

$$
\delta^{a}=p^{a}-s
$$

and the current shape of the limit order book determine the priority of execution when large market orders get executed.

For example, when a large market order to buy $Q$ stocks arrives, the $Q$ limit orders with the lowest ask prices will automatically execute. This causes a temporary market impact, since transactions occur at a price that is higher than the mid-price. If $p^{Q}$ is the price of the highest limit order executed in this trade, we define

$$
\Delta p=p^{Q}-s
$$

to be the temporary market impact of the trade of size $Q$. If our agent's limit order is within range of this market order, i.e. if $\delta^{a}<\Delta p$, his limit order will be executed.

We assume that market buy orders will "lift" our agent's sell limit orders at Poisson rate $\lambda^{a}\left(\delta^{a}\right)$, a decreasing function of $\delta^{a}$. Likewise, orders to sell stock will "hit" the agent's buy limit order at Poisson rate $\lambda^{b}\left(\delta^{b}\right)$, a decreasing function of $\delta^{b}$. Intuitively, the further away from the mid-price the agent positions his quotes, the less often he will receive buy and sell orders.

The wealth and inventory are now stochastic and depend on the arrival of market sell and buy orders. Indeed, the wealth in cash jumps every time there is a buy or sell order

$$
d X_{t}=p^{a} d N_{t}^{a}-p^{b} d N_{t}^{b}
$$

where $N_{t}^{b}$ is the amount of stocks bought by the agent and $N_{t}^{a}$ is the amount of stocks sold. $N_{t}^{b}$ and $N_{t}^{a}$ are Poisson processes with intensities $\lambda^{b}$ and $\lambda^{a}$. The number of stocks held at time $t$ is

$$
q_{t}=N_{t}^{b}-N_{t}^{a}
$$

The objective of the agent who can set limit orders is

$$
\left.u(s, x, q, t)=\max _{\delta^{a}, \delta^{b}} E_{t}\left[-\exp \left(-\gamma\left(X_{T}+q_{T} S_{T}\right)\right)\right)\right]
$$

Notice that, unlike the setting described in the previous subsection, the agent controls the bid and ask prices and therefore indirectly influences the flow of orders he receives.

Before turning to the solution of this problem, we consider some realistic functional forms for the intensities $\lambda^{a}\left(\delta^{a}\right)$ and $\lambda^{b}\left(\delta^{b}\right)$ inspired by recent results in the econophysics literature.

### 2.5 The trading intensity

One of the main focuses of the econophysics community has been to describe the laws governing the microstructure of financial markets. Here, we will be focussing on the results which address the Poisson intensity $\lambda$ with which a limit order will be executed as a function of its distance $\delta$ to the mid-price. In order to quantify this, we need to know statistics on (i) the overall frequency of market orders (ii) the distribution of their size and (iii) the temporary impact of a large market order. Aggregating these results suggests that $\lambda$ should decay as an exponential or a power law function.

For simplicity, we assume a constant frequency $\Lambda$ of market buy or sell orders. This could be estimated by dividing the total volume traded over a day by the average size of market orders on that day.

The distribution of the size of market orders has been found by several studies to obey a power law. In other word, the density of market order size is

$$
\begin{equation*}
f^{Q}(x) \propto x^{-1-\alpha} \tag{2.8}
\end{equation*}
$$

for large $x$, with $\alpha=1.53$ in Gopikrishnan et al. [4] for U.S. stocks, $\alpha=1.4$ in Maslow and Mills [9] for shares on the NASDAQ and $\alpha=1.5$ in Gabaix et al. [3] for the Paris Bourse.

There is less consensus on the statistics of the market impact in the econophysics literature. This is due to a general disagreement over how to define it and how to measure it. Some authors find that the change in price $\Delta p$ following a market order of size $Q$ is given by

$$
\begin{equation*}
\Delta p \propto Q^{\beta} \tag{2.9}
\end{equation*}
$$

where $\beta=0.5$ in Gabaix et al. [3] and $\beta=0.76$ in Weber and Rosenow [14]. Potters and Bouchaud [11] find a better fit to the function

$$
\begin{equation*}
\Delta p \propto \ln (Q) \tag{2.10}
\end{equation*}
$$

Aggregating this information, we may derive the Poisson intensity at which our agent's orders are executed. This intensity will depend only on the distance of his quotes to the mid-price, i.e. $\lambda^{b}\left(\delta^{b}\right)$ for the arrival of sell orders and $\lambda^{a}\left(\delta^{a}\right)$ for the arrival of buy orders. For instance, using (2.8) and (2.10), we derive

$$
\begin{align*}
\lambda(\delta) & =\Lambda P(\Delta p>\delta) \\
& =\Lambda P(\ln (Q)>K \delta) \\
& =\Lambda P(Q>\exp (K \delta))  \tag{2.11}\\
& =\Lambda \int_{\exp (K \delta)}^{\infty} x^{-1-\alpha} d x \\
& =A \exp (-k \delta)
\end{align*}
$$

where $A=\Lambda / \alpha$ and $k=\alpha K$. In the case of a power price impact (2.9), we obtain an intensity of the form

$$
\lambda(\delta)=B \delta^{-\frac{\alpha}{\beta}}
$$

Alternatively, since we are interested in short term liquidity, the market impact function could be derived directly by integrating the density of the limit order book. This procedure is described in Smith et al. [12] and Weber and Rosenow [14] and yields what is sometimes called the "virtual" price impact.

## 3 The solution

### 3.1 Optimal bid and ask quotes

Recall that our agent's objective is given by the value function

$$
\begin{equation*}
\left.u(s, x, q, t)=\max _{\delta^{a}, \delta^{b}} E_{t}\left[-\exp \left(-\gamma\left(X_{T}+q_{T} S_{T}\right)\right)\right)\right] \tag{3.1}
\end{equation*}
$$

where the optimal feedback controls $\delta^{a}$ and $\delta^{b}$ will turn out to be time and state dependent. This type of optimal dealer problem was first studied by Ho and Stoll [6]. One of the key steps in their analysis is to use the dynamic programming principle to show that the function $u$ solves the following Hamilton-Jacobi-Bellman equation

$$
\left\{\begin{array}{l}
u_{t}+\frac{1}{2} \sigma^{2} u_{s s}+\max _{\delta^{b}} \lambda^{b}\left(\delta^{b}\right)\left[u\left(s, x-s+\delta^{b}, q+1, t\right)-u(s, x, q, t)\right] \\
+\max _{\delta^{a}} \lambda^{a}\left(\delta^{a}\right)\left[u\left(s, x+s+\delta^{a}, q-1, t\right)-u(s, x, q, t)\right]=0 \\
u(s, x, q, T)=-\exp (-\gamma(x+q s))
\end{array}\right.
$$

The solution to this nonlinear PDE is continuous in the variables $s, x$ and $t$ and depends on the discrete values of the inventory $q$. Due to our choice of exponential utility, we are able to simplify the problem with the ansatz

$$
\begin{equation*}
u(s, x, q, t)=-\exp (-\gamma x) \exp (-\gamma \theta(s, q, t)) \tag{3.2}
\end{equation*}
$$

Direct substitution yields the following equation for $\theta$

$$
\left\{\begin{array}{l}
\theta_{t}+\frac{1}{2} \sigma^{2} \theta_{s s}-\frac{1}{2} \sigma^{2} \gamma \theta_{s}^{2}  \tag{3.3}\\
+\max _{\delta^{b}}\left[\frac{\lambda^{b}\left(\delta^{b}\right)}{\gamma}\left[1-e^{\gamma\left(s-\delta^{b}-r^{b}\right)}\right]\right]+\max _{\delta^{a}}\left[\frac{\lambda^{a}\left(\delta^{a}\right)}{\gamma}\left[1-e^{-\gamma\left(s+\delta^{a}-r^{a}\right)}\right]\right]=0 \\
\theta(s, q, T)=q s
\end{array}\right.
$$

Applying the definition of reservation bid and ask prices (given in Section 2.2) to the ansatz (3.2), we find that $r^{b}$ and $r^{a}$ depend directly on this function $\theta$. Indeed,

$$
\begin{equation*}
r^{b}(s, q, t)=\theta(s, q+1, t)-\theta(s, q, t) \tag{3.4}
\end{equation*}
$$

is the reservation bid price of the stock, when the inventory is $q$ and

$$
\begin{equation*}
r^{a}(s, q, t)=\theta(s, q, t)-\theta(s, q-1, t) \tag{3.5}
\end{equation*}
$$

is the reservation ask price, when the inventory is $q$. From the first order optimality condition in (3.9), we obtain the optimal distances $\delta^{b}$ and $\delta^{a}$. They are given by the implicit relations

$$
\begin{equation*}
s-r^{b}(s, q, t)=\delta^{b}-\frac{1}{\gamma} \ln \left(1-\gamma \frac{\lambda^{b}\left(\delta^{b}\right)}{\frac{\partial \lambda^{b}}{\partial \delta}\left(\delta^{b}\right)}\right) \tag{3.6}
\end{equation*}
$$

and

$$
\begin{equation*}
r^{a}(s, q, t)-s=\delta^{a}-\frac{1}{\gamma} \ln \left(1-\gamma \frac{\lambda^{a}\left(\delta^{a}\right)}{\frac{\partial \lambda^{a}}{\partial \delta}\left(\delta^{a}\right)}\right) \tag{3.7}
\end{equation*}
$$

In summary, the optimal bid and ask quotes are obtained through an intuitive, two step procedure. First, we solve the PDE (3.3) in order to obtain the reservation bid and ask prices $r^{b}(s, q, t)$ and $r^{a}(s, q, t)$. Second, we solve the implicit equations (3.6) and (3.7) and obtain the optimal distances $\delta^{b}(s, q, t)$ and $\delta^{a}(s, q, t)$ between the mid-price and optimal bid and ask quotes. This second step can be interpreted as a calibration of our indifference prices to the current market supply $\lambda^{b}$ and demand $\lambda^{a}$.

### 3.2 Asymptotic expansion in $q$

The main computational difficulty lies in solving equation (3.3). The order arrival terms (i.e. the terms to be maximized in the expression) are highly non-linear and may depend on the inventory. We therefore suggest an asymptotic expansion of $\theta$ in the inventory variable $q$, and a linear approximation of the order arrival terms. In the case of symmetric, exponential arrival rates

$$
\begin{equation*}
\lambda^{a}(\delta)=\lambda^{b}(\delta)=A e^{-k \delta} \tag{3.8}
\end{equation*}
$$

the indifference prices $r^{a}(s, q, t)$ and $r^{b}(s, q, t)$ coincide with their "frozen inventory" values, as described in section 2.2.

Substituting the optimal values given by equations (3.6) and (3.7) into (3.3) and using the exponential arrival rates, we obtain

$$
\left\{\begin{array}{l}
\theta_{t}+\frac{1}{2} \sigma^{2} \theta_{s s}-\frac{1}{2} \sigma^{2} \gamma \theta_{s}^{2}+\frac{A}{k+\gamma}\left(e^{-k \delta^{a}}+e^{-k \delta^{b}}\right)=0  \tag{3.9}\\
\theta(s, q, T)=q s
\end{array}\right.
$$

Consider an asymptotic expansion in the inventory variable

$$
\begin{equation*}
\theta(q, s, t)=\theta^{0}(s, t)+q \theta^{1}(s, t)+\frac{1}{2} q^{2} \theta^{2}(s, t)+\ldots \tag{3.10}
\end{equation*}
$$

The exact relations for the indifference bid and ask prices, (3.4) and (3.5), yield

$$
\begin{equation*}
r^{a}(s, q, t)=\theta^{1}(s, t)+(1-2 q) \theta^{2}(s, t)+\ldots \tag{3.11}
\end{equation*}
$$

and

$$
\begin{equation*}
r^{b}(s, q, t)=\theta^{1}(s, t)+(-1-2 q) \theta^{2}(s, t)+\ldots \tag{3.12}
\end{equation*}
$$

Using equations (3.11) and (3.12), along with the optimality conditions (3.6) and(3.7), we find that the optimal pricing strategy amounts to quoting a spread of

$$
\begin{equation*}
\delta^{a}+\delta^{b}=2 \theta^{2}(s, t)+\frac{2}{\gamma} \ln \left(1+\frac{\gamma}{k}\right) \tag{3.13}
\end{equation*}
$$

around the reservation price given by

$$
r(s, q, t)=\frac{r^{a}+r^{b}}{2}=\theta^{1}(s, t)-2 q \theta^{2}(s, t)
$$

The term $\theta^{1}$ can be interpreted as the reservation price, when the inventory is zero. The term $\theta^{2}$ may be interpreted as the sensitivity of the market maker's quotes to changes in inventory. For instance, if $\theta^{2}$ is large, accumulating a long position $q>0$ will result in aggressively low quotes.

The bid-ask spread in (3.13) is independent of the inventory. This follows from our assumption of exponential arrival rates. The spread consists of two components, one that depends on the sensitivity to changes in inventory $\theta^{2}$ and one that depends on the intensity of arrival of orders, through the parameter $k$.

Taking a first order approximation of the order arrival term

$$
\begin{equation*}
\frac{A}{k+\gamma}\left(e^{-k \delta^{a}}+e^{-k \delta^{b}}\right)=\frac{A}{k+\gamma}\left(2-k\left(\delta^{a}+\delta^{b}\right)+\ldots\right) \tag{3.14}
\end{equation*}
$$

we notice that the linear term does not depend on the inventory $q$. Therefore, if we substitute (3.10) and (3.14) into (3.9) and group terms of of order $q$, we obtain

$$
\left\{\begin{array}{l}
\theta_{t}^{1}+\frac{1}{2} \sigma^{2} \theta_{s s}^{1}=0  \tag{3.15}\\
\theta^{1}(s, T)=s
\end{array}\right.
$$

whose solution is $\theta^{1}(s, t)=s$. Grouping terms of order $q^{2}$ yields

$$
\left\{\begin{array}{l}
\theta_{t}^{2}+\frac{1}{2} \sigma^{2} \theta_{s s}^{2}-\frac{1}{2} \sigma^{2} \gamma\left(\theta_{s}^{1}\right)^{2}=0  \tag{3.16}\\
\theta^{2}(s, T)=0
\end{array}\right.
$$

whose solution is $\theta^{2}=\frac{1}{2} \sigma^{2} \gamma(T-t)$. Thus, for this linear approximation of the order arrival term, we obtain the same indifference price

$$
\begin{equation*}
r(s, t)=s-q \gamma \sigma^{2}(T-t) \tag{3.17}
\end{equation*}
$$

as for the "frozen inventory" problem from section 2.2. We then set a bid/ask spread given by

$$
\begin{equation*}
\delta^{a}+\delta^{b}=\gamma \sigma^{2}(T-t)+\frac{2}{\gamma} \ln \left(1+\frac{\gamma}{k}\right) \tag{3.18}
\end{equation*}
$$

around this indifference or reservation price. Note that if we had taken a quadratic approximation of the order arrival term, we would still obtain $\theta^{1}=s$, but the sensitivity term $\theta^{2}(s, t)$ would solve a non-linear PDE.

Equations (3.17) and (3.18) thus provide us with simple expressions for the bid and ask prices in terms of our model parameters. This approximate solution also simplifies the simulations we perform in the next section.

### 3.3 Numerical simulations

We now test the performance of our strategy, focussing primarily on the shape of the P\&L profile and the final inventory $q_{T}$. We will refer to our strategy as the "inventory" strategy, and compare it to a benchmark strategy that is symmetric around the midprice, regardless of the inventory. This strategy, which we refer to as the "symmetric" strategy, uses the same spread as the inventory strategy, but centers it around the mid-price, rather than the reservation price.

In practice, the choice of time step $d t$ is a subtle one. On the one hand, $d t$ must be small enough so that the probability of multiple orders reaching our agent is small. On the other hand, $d t$ must be larger than the typical tick time, otherwise the agent's quotes will be updated so frequently that he will not see any orders (particularly if his quotes are outside the market bid/ask spread).

As far as our simulation is concerned, we chose the following parameters: $s=100$, $T=1, \sigma=2, d t=0.005, q=0, \gamma=0.1, k=1.5$ and $A=140$. The simulation is obtained through the following procedure: at time $t$, the agent's quotes $\delta^{a}$ and $\delta^{b}$ are computed, given the state variables. At time $t+d t$, the state variables are updated. With probability $\lambda^{a}\left(\delta^{a}\right) d t$, the inventory variable decreases by one and the wealth increases by $s+\delta^{a}$. With probability $\lambda^{b}\left(\delta^{b}\right) d t$, the inventory increases by one and the wealth decreases by $s-\delta^{b}$. The mid-price is updated by a random increment $\pm \sigma \sqrt{d t}$. Figure 1 illustrates the bid and ask quotes for one simulation of a stock path.

![](https://cdn.mathpix.com/cropped/2025_10_26_d34e60f603b0b57ea2eeg-11.jpg?height=695&width=1193&top_left_y=352&top_left_x=363)
Figure 1. The mid-price, indifference price and the optimal bid and ask quotes

Notice that, at time $t=0.25$, the indifference price is higher than the mid-price, indicating that the inventory position must be negative (or short stock). Since the bid price is aggressively placed near the mid-price, our agent is more likely to buy stock and the inventory quickly returns to zero by time $t=0.3$. As we approach the terminal time, the indifference price gets closer to the mid-price, and our agent's bid/ask quotes look more like a symmetric strategy. Indeed, when we are close to the terminal time, our inventory position is considered less risky, since the mid-price is less likely to move drastically.

We then run 1000 simulations to compare our "inventory" strategy to the "symmetric" strategy. This strategy uses the same bid/ask spread as the inventory strategy, but centers it around the mid-price. For example, the performance of the symmetric strategy that quotes a bid/ask spread of $\$ 1.29$ (corresponding to the optimal agent with $\gamma=0.1$ ) is displayed in Table 1. This symmetric strategy has a higher return and higher standard deviation than the inventory strategy. The symmetric strategy obtains a slightly higher return since it is centered around the mid-price, and therefore receives a higher volume of orders than the inventory strategy. However, the inventory strategy obtains a P\&L profile with a much smaller variance, as illustrated in the histogram in Figure 2.

| Strategy | Spread | Profit | std(Profit) | Final q | std(Final q) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| Inventory | 1.29 | 62.94 | 5.89 | 0.10 | 2.80 |
| Symmetric | 1.29 | 67.21 | 13.43 | -0.018 | 8.66 |

Table 1: 1000 simulations with $\gamma=0.1$

![](https://cdn.mathpix.com/cropped/2025_10_26_d34e60f603b0b57ea2eeg-12.jpg?height=576&width=703&top_left_y=349&top_left_x=661)
Figure 2. $\gamma=0.1$

The results of the simulations comparing the "inventory" strategy for $\gamma=0.01$ with the corresponding "symmetric" strategy are displayed in Table 2. This small value for $\gamma$ represents an investor who is close to risk neutral. The inventory effect is therefore much smaller and the P\&L profiles of the two strategies are very similar, as illustrated in Figure 3. In fact, in the limit as $\gamma \rightarrow 0$ the two strategies are identical.

| Strategy | Spread | Profit | std(Profit) | Final q | std(Final q) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| Inventory | 1.33 | 66.78 | 8.76 | -0.02 | 4.70 |
| Symmetric | 1.33 | 67.36 | 13.40 | -0.31 | 8.65 |

Table 2: 1000 simulations with $\gamma=0.01$

Finally, we display the performance of the two strategies for $\gamma=0.5$ in Table 3. This choice corresponds to a very risk averse investor, who will go to great lengths to avoid accumulating an inventory. This strategy produces low standard deviations of profits and final inventory, but generates more modest profits than the corresponding symmetric strategy (see Figure 4).

| Strategy | Spread | Profit | std(Profit) | Final q | std(Final q) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| Inventory | 1.15 | 33.92 | 4.72 | -0.02 | 1.88 |
| Symmetric | 1.15 | 66.20 | 14.53 | 0.25 | 9.06 |

Table 3: 1000 simulations with $\gamma=0.5$

![](https://cdn.mathpix.com/cropped/2025_10_26_d34e60f603b0b57ea2eeg-13.jpg?height=579&width=698&top_left_y=346&top_left_x=243)
Figure 3. $\gamma=0.01$

![](https://cdn.mathpix.com/cropped/2025_10_26_d34e60f603b0b57ea2eeg-13.jpg?height=576&width=703&top_left_y=349&top_left_x=1084)
Figure 4. $\gamma=0.5$

## Appendix

Herein, we consider the geometric Brownian motion

$$
\frac{d S_{u}}{S_{u}}=\sigma d W_{u}
$$

with initial value $S_{t}=s$, and the mean/variance objective

$$
V(x, s, q, t)=E_{t}\left[\left(x+q S_{T}\right)-\frac{\gamma}{2}\left(q S_{T}-q s\right)^{2}\right]
$$

where $x$ is the initial wealth in dollars. This value function can be written as

$$
V(x, s, q, t)=x+q s+\frac{\gamma q^{2} s^{2}}{2}\left(e^{\sigma^{2}(T-t)}-1\right)
$$

This yields reservation prices of the form

$$
R^{a}(s, q, t)=s+\frac{(1-2 q)}{2} \gamma s^{2}\left(e^{\sigma^{2}(T-t)}-1\right)
$$

and

$$
R^{b}(s, q, t)=s+\frac{(-1-2 q)}{2} \gamma s^{2}\left(e^{\sigma^{2}(T-t)}-1\right) .
$$

These results are analogous to the ones obtained in section 2.2.

## References

[1] B. Biais, L. Glosten and C. Spatt, The Microstructure of Stock Markets, Working paper (2004).
[2] J.-P. Bouchaud, M. Mezard and M. Potters, Statistical Properties of Stock Order Books: Empirical Results and Models, Quantitative Finance, 2 (2002) 251-256.
[3] X. Gabaix, P. Gopikrishnan, V. Plerou, H.E. Stanley, A Theory of Large Fluctuations in Stock Market Activity, MIT Department of Economics Working Paper No. 03-30.
[4] P. Gopikrishnan, V. Plerou, X. Gabaix and H.E. Stanley, Statistical Properties of Share Volume Traded in Financial Markets, Physical Review E, 62 (2000) R4493-R4496.
[5] T. Ho and R. Macris, Dealer Bid-Ask Quotes and Transaction Prices: An Empirical Study of Some AMEX Options, Journal of Finance, 39, (1984) 23-45.
[6] T. Ho and H. Stoll, Optimal Dealer Pricing under Transactions and Return Uncertainty, Journal of Financial Economics, 9 (1981) 47-73.
[7] T. Ho and H. Stoll, On Dealer Markets under Competition, Journal of Finance, 35 (1980) 259-267.
[8] H. Luckock, A Steady-State Model of the Continuous Double Auction, Quantitative Finance, 3 (2003) 385-404.
[9] S. Maslow and M. Mills, Price Fluctuations from the Order Book Perspective: Empirical Facts and a Simple Model, Physica A, 299 (2001) 234-246.
[10] M. O'Hara, Market Microstructure Theory, Blackwell, Cambridge, Massachusetts (1997).
[11] M. Potters and J.-P. Bouchaud, More Statistical Properties of Order Books and Price Impact, Physica A: Statistical Mechanics and its Applications, 324 (2003) 133-140.
[12] E. Smith, J.D. Farmer, L. Gillemot and S. Krishnamurthy, Statistical Theory of the Continuous Double Auction, Quantitative Finance, 3 (2003) 481-514.
[13] H.R. Stoll, Market Microstructure, Handbook of the Economics of Finance, Edited by G.M. Constantinides et al. North Holland (2003).
[14] P. Weber and B. Rosenow, Order Book Approach to Price Impact, Quantitative Finance, 5 (2005) 357-364.


[^0]:    ${ }^{1}$ We choose this model over the standard geometric Brownian motion to ensure that the utility functionals introduced in the sequel remain bounded. In practical applications, we could also use a dimensionless model such as

    $$
    \begin{equation*}
    \frac{d S_{u}}{S_{u}}=\sigma d W_{u} \tag{2.2}
    \end{equation*}
    $$

    with inital value $S_{t}=s$. To avoid mathematical infinities, exponential utility functions could be modified to a standard mean/variance objective with the same Taylor-series expansion. The essence of the results would remain. More details regarding the model (2.2) with mean/variance utility are given in the appendix.

