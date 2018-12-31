using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillY1b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject MyLineObj;
    public GameObject MyLine;
    public float maxdistance = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    public float speed;
    public float damage;
    public float maxtime;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void Update()
    {
        if (Input.GetButtonDown("FireY") && skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            if (MyLine != null)
                MyLine.GetComponent<DestroyScript>().Destroyself();
            MyLine = Instantiate(MyLineObj, gameObject.transform.position, Quaternion.identity);
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        float realdistance = Mathf.Min((float)skilldirection.Length(), maxdistance);
        if (realdistance <= 0.6)
        {
            return;
        }   //半径小于自身半径时不施法
        Fix64Vector2 realplace = singplace + skilldirection.normalized() * (Fix64)realdistance;
        Vector2 rpv2 = realplace.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        if (Physics2D.OverlapPoint(rpv2))
        {
            Collider2D hit = Physics2D.OverlapPoint(rpv2);
            if (hit.GetComponent<HPScript>() != null)
            {
                //MyLine.GetComponent<RedLineScript>().DoMyJob(hit.gameObject.GetPhotonView().photonView.viewID, gameObject.GetPhotonView().viewID, realplace, speed, damage, maxtime);
                return;
            }
        }
        //MyLine.GetComponent<RedLineScript>().DoMyJob(0, gameObject.GetPhotonView().viewID, realplace, speed, damage, maxtime);
    }
}
