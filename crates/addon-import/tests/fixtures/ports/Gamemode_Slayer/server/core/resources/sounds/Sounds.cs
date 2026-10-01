// Stand-in (CC0) with the original's shape: a countdown voice per second,
// made in a loop, and the buzzer at the start.
$Slayer::Server::Sounds::MaxCountDown = 10;

for($sound = 1; $sound <= $Slayer::Server::Sounds::MaxCountDown; $sound ++)
{
	datablock AudioProfile(slayerSound)
	{
		filename = "./" @ $sound @ ".wav";
		description = AudioClosest3d;
		preload = false;
	};
	slayerSound.setName("Slayer_" @ $sound @ "_Seconds_Sound");
}

datablock AudioProfile(Slayer_Begin_Sound)
{
	filename = "./buzzer.wav";
	description = AudioClosest3d;
	preload = false;
};
