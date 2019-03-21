using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class MessageListScript : MonoBehaviour
{
    public Text blackboard;

    public void writeblacklist(CSteamID cSteamID, string word)
    {
        blackboard.text += ("\n" + SteamFriends.GetFriendPersonaName(cSteamID) + ":\n" + word);
    }

    public void clearblackboard()
    {
        blackboard.text = "";
    }
}
