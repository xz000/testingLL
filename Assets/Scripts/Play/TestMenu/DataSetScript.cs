using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class DataSetScript : MonoBehaviour
{
    public Text DataInput;
    public string DataName;

    public void SetMyData()
    {
        if (!isActiveAndEnabled)
            return;
        gameObject.SetActive(false);
        SteamMatchmaking.SetLobbyData(Sender.roomid, DataName, DataInput.text);
    }
}
