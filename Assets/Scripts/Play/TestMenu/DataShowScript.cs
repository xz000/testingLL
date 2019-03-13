using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class DataShowScript : MonoBehaviour
{
    public Text DataValueText;
    public string DataName;
    public GameObject MPanel;
    public GameObject SetObject;

    public void GetMyData()
    {
        DataValueText.text = SteamMatchmaking.GetLobbyData(Sender.roomid, DataName);
        if (MPanel.activeInHierarchy)
        {
            SetObject.SetActive(true);
        }
    }
}
