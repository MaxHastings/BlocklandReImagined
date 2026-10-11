`Hello::kept` (`KeptWorld`: the map and replication cursor a resuming
client's bricks are current to) and `Checkpoint::kept` (`KeptChanges`: the
bricks removed since that cursor). With both, a Welcome's `WorldChunk`
frames carry only the bricks added or changed since the drop instead of the
whole world; without a match the host sends the whole world as before.
