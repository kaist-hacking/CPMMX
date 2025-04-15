# crawl_pairs

This directory contains the codes which fetch all tokens from the given Uniswap v2 factory and filtering them by their volume.

## `0.fetch_pairs.py`

This code fetches PancakeSwap pairs from the `allPairs` array of the Uniswap v2 factory.

The factory address is hardcoded. You need to specify the start/end of the fetching range;
you may set the start as zero and end as the [`allPairsLength`](https://bscscan.com/address/0xca143ce32fe78f1f7019d7d551a6402fc5350c73#readContract#F3)
of your factory contract.

```
$ python 0.fetch_pairs.py OUTPUT_FILE_PATH
```

## `1.filter_volume.py`

This code filters the list of pair from the above code, by the volume of that pair.

```
$ python 1.filter_volume.py 20240417_allpairs.csv OUTPUT_FILE_PATH
```

The prices of each tokens and the threshold for filtering are hardcoded in the code.