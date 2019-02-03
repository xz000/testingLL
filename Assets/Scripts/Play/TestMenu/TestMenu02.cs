using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class TestMenu02 : MonoBehaviour
{
    public GameObject Startbutton;
    public GameObject Masterpanel;
    public GameObject Otherpanel;
    public Toggle Readytoggle;
    public Toggle AutostartToggle;

    public Text PlayersJoined;
    public Text Notready;
    public Text Roomname;
    public CSteamID roomid;

    protected Callback<LobbyKicked_t> Callback_LobbyKicked;
    protected Callback<LobbyChatUpdate_t> Callback_LobbyChatUpdate;
    protected Callback<LobbyDataUpdate_t> Callback_LobbyDataUpdate;

    ///public GameObject Menu00;
    public GameObject Menu01;

    bool isready = false;

    /*public override void OnJoinedRoom()
    {
        setreadystatusonline();
        showpanel();
        Roomname.text = PhotonNetwork.room.Name;
        if (PhotonNetwork.isMasterClient && !PhotonNetwork.room.IsOpen)
            PhotonNetwork.room.IsOpen = true;
    }*/
    void Update()
    {
        SteamAPI.RunCallbacks();
    }
    
    private void Start()
    {
        Callback_LobbyKicked = Callback<LobbyKicked_t>.Create(OnLobbyKicked);
        Callback_LobbyChatUpdate = Callback<LobbyChatUpdate_t>.Create(OnLobbyChatUpdate);
        Callback_LobbyKicked = Callback<LobbyKicked_t>.Create(OnLobbyKicked);
    }

    void OnEnable()
    {
        Roomname.text = SteamMatchmaking.GetLobbyData(roomid, "name");
        PlayersJoined.text = SteamMatchmaking.GetNumLobbyMembers(roomid) + " players joined";
    }

    void OnLobbyKicked(LobbyKicked_t lobbyKicked_T)
    {
        roomid = (CSteamID)lobbyKicked_T.m_ulSteamIDLobby;
        SwitchToMenu01();
    }

    void OnLobbyDataUpdate(LobbyDataUpdate_t lobbyDataUpdate_T)
    {
        roomid = (CSteamID)lobbyDataUpdate_T.m_ulSteamIDLobby;
        Roomname.text = SteamMatchmaking.GetLobbyData(roomid, "name");
    }

    void OnLobbyChatUpdate(LobbyChatUpdate_t lobbyChatUpdate_T)
    {
        roomid = (CSteamID)lobbyChatUpdate_T.m_ulSteamIDLobby;
        PlayersJoined.text = SteamMatchmaking.GetNumLobbyMembers(roomid) + " players joined";
    }

    void LeaveLobby()
    {
        SteamMatchmaking.LeaveLobby(roomid);
        SwitchToMenu01();
    }

    public void SwitchToMenu01()
    {
        gameObject.SetActive(false);
        Menu01.SetActive(true);
    }

    /*public override void OnPhotonPlayerConnected(PhotonPlayer newPlayer)
    {
        Updatetext();
    }

    public override void OnPhotonPlayerDisconnected(PhotonPlayer otherPlayer)
    {
        Updatetext();
    }

    public override void OnMasterClientSwitched(PhotonPlayer newMasterClient)
    {
        showpanel();
    }*/

    public void showmasterpanel()
    {
        Otherpanel.SetActive(false);
        Masterpanel.SetActive(true);
        isready = true;
        setreadystatusonline();
    }

    public void showotherpanel()
    {
        Masterpanel.SetActive(false);
        Otherpanel.SetActive(true);
        isready = Readytoggle.isOn;
        setreadystatusonline();
    }

    public void OnReadyToggleChanged()
    {
        isready = Readytoggle.isOn;
        setreadystatusonline();
    }

    public void setreadystatusonline()
    {
        /*PhotonNetwork.player.SetCustomProperties(new ExitGames.Client.Photon.Hashtable()
        {
        {"ready", isready }
        });
        photonView.RPC("Updatetext", PhotonTargets.All);*/
    }

    public void ClickStartButton()
    {
        ///PhotonNetwork.room.IsOpen = false;
        ///EnterCircleSence();
        //photonView.RPC("EnterCircleSence", PhotonTargets.All);
    }

    public void ClickBackButton()
    {
        LeaveLobby();
    }

    public void showpanel()
    {
        /*if (PhotonNetwork.isMasterClient)
            showmasterpanel();
        else
            showotherpanel();*/
    }

    public void Updatetext()
    {
        /*PlayersJoined.text = PhotonNetwork.playerList.Length + " players joined";
        int i = 0;
        foreach (PhotonPlayer pp in PhotonNetwork.playerList)
        {
            if ((bool)pp.CustomProperties["ready"] == false)
                i++;
        }
        Notready.text = i + " not ready";
        if (PhotonNetwork.isMasterClient)
        {
            if (i == 0)
            {
                Startbutton.SetActive(true);
                if (AutostartToggle.isOn && PhotonNetwork.playerList.Length > 1)
                    ClickStartButton();
            }
            else
                Startbutton.SetActive(false);
        }*/
    }
}
