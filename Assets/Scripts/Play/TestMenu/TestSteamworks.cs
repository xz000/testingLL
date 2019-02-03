using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class TestSteamworks : MonoBehaviour
{
    public GameObject Menu01;
    public GameObject Menu02;

    // Use this for initialization
    void Start ()
    {
        ///SteamAPI.Init();
        /*if (PhotonNetwork.room.Name != null)
        {
            Menu01.SetActive(false);
            Menu02.SetActive(true);
        }
        else*/
        {
            Menu01.SetActive(true);
            Menu02.SetActive(false);
        }
        ///DontDestroyOnLoad(gameObject);
    }
}