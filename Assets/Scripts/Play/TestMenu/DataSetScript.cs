using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class DataSetScript : MonoBehaviour
{
    public Text DataInput;
    public Text DataNow;
    public string DataName;

    public void SetMyData()
    {
        GameObject.Find("Main Camera").GetComponent<UISound>().PlayClickSound();
        SetData();
    }

    private void OnEnable()
    {
        SetData();
    }

    private void SetData()
    {
        if (!isActiveAndEnabled)
            return;
        gameObject.GetComponent<CanvasGroup>().interactable = false;
        SteamMatchmaking.SetLobbyData(Sender.roomid, DataName, DataInput.text);
    }
}
