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
    public Text Uword;

    public void HomeWork(CSteamID tu)
    {
        theUser = tu;
        GetUserAvatar();
        Uname.text = SteamFriends.GetFriendPersonaName(tu);
        Uready.text = SteamMatchmaking.GetLobbyMemberData(Sender.roomid, tu, "key_ready");
    }

    public bool ido(CSteamID tu)
    {
        return (tu == theUser);
    }

    public void ClassWork(string s)
    {
        Uword.text = s;
        GameObject.Find("MessageListScroll").GetComponent<MessageListScript>().writeblacklist(theUser, s);
    }

    void GetUserAvatar()
    {
        int ret = SteamFriends.GetLargeFriendAvatar(theUser);
        UAI.texture = UserListScript.GetSteamImageAsTexture2D(ret);
    }
}
