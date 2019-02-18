using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class ShowMyInfo : MonoBehaviour
{
    public GameObject MyInfoText;
    GameObject MyInfo;
    GameObject MyInfoHP;
    Rigidbody2D rb2d;
    HPScript hps;

    // Use this for initialization
    void Start()
    {
        GameObject TheCanvas = GameObject.Find("Canvas2");
        MyInfo = GameObject.Instantiate(MyInfoText, TheCanvas.transform);
        MyInfoHP = GameObject.Instantiate(MyInfoText, TheCanvas.transform);
        rb2d = GetComponent<Rigidbody2D>();
        hps = GetComponent<HPScript>();
    }

    private void Update()
    {
        MyInfo.transform.position = Camera.main.WorldToScreenPoint(transform.position + Vector3.down * 0.2f);
        MyInfoHP.transform.position = Camera.main.WorldToScreenPoint(transform.position + Vector3.up * 0.2f);
        MyInfo.GetComponent<Text>().text = rb2d.position.x + "," + rb2d.position.y;
        MyInfoHP.GetComponent<Text>().text = hps.currentHP.ToString();
    }

    private void OnDestroy()
    {
        Destroy(MyInfo);
        Destroy(MyInfoHP);
    }
}
