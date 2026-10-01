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
$Slayer::Server::CTF::flagHasLight[$Slayer::Server::CTF::numFlags] = 1;
$Slayer::Server::CTF::flagLightType[$Slayer::Server::CTF::numFlags] = "ConstantLight";
$Slayer::Server::CTF::flagLightTime[$Slayer::Server::CTF::numFlags] = 500;
$Slayer::Server::CTF::flagLightRadius[$Slayer::Server::CTF::numFlags] = 12;
$Slayer::Server::CTF::flagIdleAnimation[$Slayer::Server::CTF::numFlags] = "wave";
$Slayer::Server::CTF::numFlags ++;

// Four more flag types, each with its own made-up shape and placement.
$Slayer::Server::CTF::flagName[$Slayer::Server::CTF::numFlags] = "Stand-in Stick";
$Slayer::Server::CTF::flagShapeFile[$Slayer::Server::CTF::numFlags] = "./flag1.dts";
$Slayer::Server::CTF::flagMountPoint[$Slayer::Server::CTF::numFlags] = $BackSlot;
$Slayer::Server::CTF::flagRotation[$Slayer::Server::CTF::numFlags] = eulerToMatrix("5 0 45");
$Slayer::Server::CTF::flagOffset[$Slayer::Server::CTF::numFlags] = "0.2 -0.1 -0.4";
$Slayer::Server::CTF::numFlags ++;

$Slayer::Server::CTF::flagName[$Slayer::Server::CTF::numFlags] = "Stand-in Case";
$Slayer::Server::CTF::flagShapeFile[$Slayer::Server::CTF::numFlags] = "./flag2.dts";
$Slayer::Server::CTF::flagMountPoint[$Slayer::Server::CTF::numFlags] = $BackSlot;
$Slayer::Server::CTF::flagRotation[$Slayer::Server::CTF::numFlags] = eulerToMatrix("90 0 0");
$Slayer::Server::CTF::flagOffset[$Slayer::Server::CTF::numFlags] = "0 -0.3 -0.5";
$Slayer::Server::CTF::numFlags ++;

$Slayer::Server::CTF::flagName[$Slayer::Server::CTF::numFlags] = "Stand-in Orb";
$Slayer::Server::CTF::flagShapeFile[$Slayer::Server::CTF::numFlags] = "./flag3.dts";
$Slayer::Server::CTF::flagMountPoint[$Slayer::Server::CTF::numFlags] = $BackSlot;
$Slayer::Server::CTF::flagRotation[$Slayer::Server::CTF::numFlags] = eulerToMatrix("0 45 0");
$Slayer::Server::CTF::flagOffset[$Slayer::Server::CTF::numFlags] = "0.1 0 0.1";
$Slayer::Server::CTF::numFlags ++;

$Slayer::Server::CTF::flagName[$Slayer::Server::CTF::numFlags] = "Stand-in Ball";
$Slayer::Server::CTF::flagShapeFile[$Slayer::Server::CTF::numFlags] = "./flag4.dts";
$Slayer::Server::CTF::flagMountPoint[$Slayer::Server::CTF::numFlags] = $BackSlot;
$Slayer::Server::CTF::flagRotation[$Slayer::Server::CTF::numFlags] = eulerToMatrix("0 0 0");
$Slayer::Server::CTF::flagOffset[$Slayer::Server::CTF::numFlags] = "0 0 0";
$Slayer::Server::CTF::numFlags ++;

exec("./game-mode.cs");
