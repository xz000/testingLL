using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class UserDetailScript : MonoBehaviour
{
    public CSteamID theUser;
    public RawImage UAI;
    public Text Uname;
    public Text Uready;

    public void HomeWork(CSteamID tu)
    {
        theUser = tu;
        GetUserAvatar();
        Uname.text = SteamFriends.GetFriendPersonaName(tu);
    }

    void GetUserAvatar()
    {
        int ret = SteamFriends.GetLargeFriendAvatar(theUser);
        UAI.texture = UserListScript.GetSteamImageAsTexture2D(ret);
    }
}
