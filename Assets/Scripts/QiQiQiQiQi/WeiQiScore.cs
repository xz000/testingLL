using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class WeiQiScore : MonoBehaviour
{
    public bool IBlack;
    public Color HeColor;
    public SpriteRenderer srDa;
    public SpriteRenderer srZhong;
    public SpriteRenderer srXiao;
    public WeiQi qipan;
    public int meScore = 0;
    public GameObject MyInfoText;
    GameObject MyInfo;

    // Use this for initialization
    void Start()
    {
        srDa.color = HeColor;
        GameObject TheCanvas = GameObject.Find("Canvas");
        MyInfo = GameObject.Instantiate(MyInfoText, TheCanvas.transform);
        MyInfo.transform.position = Camera.main.WorldToScreenPoint(transform.position);
        MyInfo.GetComponent<Text>().text = meScore.ToString();
    }

    private void OnDestroy()
    {
        Destroy(MyInfo);
    }

    private void OnMouseEnter()
    {
        srDa.color = Color.red;
    }

    private void OnMouseExit()
    {
        srDa.color = HeColor;
    }

    public void mePlusone()
    {
        meScore++;
        MyInfo.GetComponent<Text>().text = meScore.ToString();
    }
}
