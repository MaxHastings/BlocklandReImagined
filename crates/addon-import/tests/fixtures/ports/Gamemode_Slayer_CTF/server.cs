// Stand-in for the Gamemode_Slayer_CTF port tests (CC0). The flag table has
// the original's layout with this stand-in's own values.
// The base game defines $BackSlot; set here so the test needs no game files.
$BackSlot = 4;

$Slayer::Server::CTF::numFlags = 0;
$Slayer::Server::CTF::flagImageSlot = 2;

$Slayer::Server::CTF::flagName[$Slayer::Server::CTF::numFlags] = "Stand-in Flag";
$Slayer::Server::CTF::flagShapeFile[$Slayer::Server::CTF::numFlags] = "./flag.dts";
$Slayer::Server::CTF::flagMountPoint[$Slayer::Server::CTF::numFlags] = $BackSlot;
$Slayer::Server::CTF::flagRotation[$Slayer::Server::CTF::numFlags] = eulerToMatrix("10 0 90");
$Slayer::Server::CTF::flagOffset[$Slayer::Server::CTF::numFlags] = "0.1 -0.2 -0.3";
$Slayer::Server::CTF::numFlags ++;

exec("./game-mode.cs");
