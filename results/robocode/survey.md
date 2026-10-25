# RoboRumble survey: every robot against the Robocode champion

`royale_survey bots/rumble models/champions/gru_es_robocode.json 10 10`: each robot of the RoboRumble 1-vs-1 participant list (fetched with `scripts/fetch-rumble-robots.sh`) plays one 10-round battle against [`gru_es_robocode.json`](../../models/champions/gru_es_robocode.json), on 800 x 600 through our server, inside bubblewrap. Raw records: [`survey.jsonl`](survey.jsonl).

- **1217 of 1223 robots ran.** Our champion won **5486 of 12170 rounds (45.1%)**.
- 660 robots beat it; 233 won all 10 rounds. It won all 10 against 199.
- 30 ran but never dealt damage or won a round: some are simply harmless, some break under the bridge. BeepBoop, the RoboRumble leader, also never dealt damage; it fails the same way on the official server (its enemy tracking never starts), so the bridge is at fault, not our server.
- Did not run: four need Java 21 (Aristocles, Guessit, Pugilist, Solveit; this machine has Java 17), FireDragon's jar lacks its class, and SkippedTurns, which skips turns on purpose, timed out. Kotlin robots (Mirage, Ronin, Omni) need the Kotlin standard library, which the import script now adds.
- [`rumble_split.json`](rumble_split.json) splits the 1180 robots usable for training (they did something and answer in under 6 ms a turn) into 787 for training and 393 held out, of equal strength: the champion took 43.7% of the rounds against each half.

## The strongest against our champion

| Robot | Rounds (robot–ours) | Damage dealt / taken |
|---|---|---|
| kc.serpent.Hydra_0.21 | 10–0 | 624 / 14 |
| pla.Memnoch_0.5 | 10–0 | 951 / 25 |
| awesomeness.Elite_1.0 | 10–0 | 886 / 24 |
| oog.micro.SavantMicro_1.1 | 10–0 | 907 / 36 |
| wcsv.Engineer.Engineer_0.5.4 | 10–0 | 558 / 35 |
| mld.Wisdom_1.0 | 10–0 | 956 / 75 |
| PK.Twardy_0.4.2 | 10–0 | 740 / 60 |
| blir.micro.blixi.Blixi_1.2 | 10–0 | 836 / 70 |
| jk.nano.Machete_2.0 | 10–0 | 748 / 64 |
| simonton.GFNano_D_3.1b | 10–0 | 886 / 76 |
| fromHell.CHCl3_1.4.2 | 10–0 | 685 / 59 |
| buba.Archivist_0.1 | 10–0 | 865 / 77 |
| robar.micro.Kirbyi_1.0 | 10–0 | 873 / 80 |
| rh.Intra_2.1DomoKN | 10–0 | 785 / 72 |
| nat.Hikari_dev0001 | 10–0 | 175 / 16 |
| mn.micro.perceptual.Mimic_1.0.0 | 10–0 | 492 / 47 |
| trab.nano.AinippeNano_1.3 | 10–0 | 898 / 87 |
| wiki.mini.Griffon_0.1 | 10–0 | 852 / 83 |
| tjk.deBroglie_rev0108 | 10–0 | 748 / 73 |
| robar.nano.Mosquito_1.1 | 10–0 | 864 / 91 |
| cjm.Che_1.2 | 10–0 | 496 / 52 |
| jekl.Jekyl_.70 | 10–0 | 851 / 92 |
| exauge.LemonDrop_1.6.130 | 10–0 | 904 / 100 |
| jk.mini.CunobelinDC_1.2 | 10–0 | 663 / 74 |
| fromHell.C22H30N2O2S_2.2 | 10–0 | 874 / 98 |
